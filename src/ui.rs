//! 설정 창 (egui)

use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::config::{APP_NAME, APP_VERSION, Config, MouseButton, TextMethod, WebButton, available_keys};
use crate::{Shared, autostart, service, update};

const ICON_PNG: &[u8] = include_bytes!("../assets/gettext.png");

pub fn run_settings(shared: Arc<Shared>) {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(format!("{APP_NAME} {APP_VERSION}"))
        .with_inner_size([760.0, 640.0])
        .with_min_inner_size([600.0, 420.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON_PNG) {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };

    let result = eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            if let Ok(handle) = cc.window_handle()
                && let RawWindowHandle::Win32(h) = handle.as_raw()
            {
                shared.ui_hwnd.store(h.hwnd.get(), Ordering::SeqCst);
            }
            *shared.egui_ctx.lock().unwrap() = Some(cc.egui_ctx.clone());
            Ok(Box::new(SettingsApp::new(shared)))
        }),
    );
    if let Err(e) = result {
        eprintln!("settings window error: {e}");
    }
}

/// 기본 폰트에 한글이 없으므로 맑은 고딕을 우선 폰트로 추가
fn setup_fonts(ctx: &egui::Context) {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let path = std::path::Path::new(&windir).join("Fonts").join("malgun.ttf");
    let Ok(bytes) = std::fs::read(path) else { return };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("malgun".into(), Arc::new(egui::FontData::from_owned(bytes)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().insert(0, "malgun".into());
    }
    ctx.set_fonts(fonts);
}

enum Message {
    Info(String),
    Error(String),
}

/// 명령 버튼 목록 편집 동작
enum ButtonAction {
    Up(usize),
    Down(usize),
    Remove(usize),
}

struct SettingsApp {
    shared: Arc<Shared>,
    keys: Vec<String>,
    /// 편집 중인 설정 (저장 전 사본)
    draft: Config,
    /// 마지막으로 저장된 설정 (변경 여부 판단용)
    original: Config,
    message: Option<Message>,
    first_frame: bool,
    /// Windows 시작 시 자동 실행 여부 (레지스트리 기준)
    autostart: bool,
    /// 제외 프로그램 입력란
    new_excluded: String,
}

impl SettingsApp {
    fn new(shared: Arc<Shared>) -> Self {
        let config = shared.config.lock().unwrap().clone();
        Self {
            shared,
            keys: available_keys(),
            draft: config.clone(),
            original: config,
            message: None,
            first_frame: true,
            autostart: autostart::is_enabled(),
            new_excluded: String::new(),
        }
    }

    fn is_dirty(&self) -> bool {
        self.draft != self.original
    }

    /// 업데이트 관련 설정만 즉시 저장 (편집 중인 다른 설정은 건드리지 않음)
    fn save_update_settings(&mut self, edit: impl Fn(&mut Config)) -> Result<(), String> {
        let mut cfg = self.shared.config.lock().unwrap();
        let mut next = cfg.clone();
        edit(&mut next);
        next.save().map_err(|e| format!("설정 파일 저장 실패: {e}"))?;
        *cfg = next;
        edit(&mut self.draft);
        edit(&mut self.original);
        Ok(())
    }

    fn save_draft(&mut self) {
        if let Err(e) = self.draft.validate() {
            self.message = Some(Message::Error(e));
            return;
        }
        let result = {
            let mut cfg = self.shared.config.lock().unwrap();
            let mut next = self.draft.clone();
            next.excluded_programs = next.normalized_excluded();
            next.check_updates = cfg.check_updates;
            next.skipped_version = cfg.skipped_version.clone();
            match next.save() {
                Ok(()) => {
                    *cfg = next.clone();
                    Ok(next)
                }
                Err(e) => Err(format!("설정 파일 저장 실패: {e}")),
            }
        };
        match result {
            Ok(saved) => {
                self.draft = saved.clone();
                self.original = saved;
                self.message = Some(Message::Info("저장했습니다.".into()));
                self.shared.request_reload();
            }
            Err(e) => self.message = Some(Message::Error(e)),
        }
    }

    fn toggle_check_updates(&mut self, enabled: bool) {
        match self.save_update_settings(|c| c.check_updates = enabled) {
            Ok(()) => {
                if enabled {
                    self.shared.post_service(service::WM_APP_CHECK_UPDATE, 0);
                }
            }
            Err(e) => self.message = Some(Message::Error(e)),
        }
    }

    fn skip_update(&mut self, info: &update::UpdateInfo) {
        let tag = info.tag.clone();
        if let Err(e) = self.save_update_settings(|c| c.skipped_version = Some(tag.clone())) {
            self.message = Some(Message::Error(e));
            return;
        }
        self.shared.update.lock().unwrap().available = None;
        self.shared.post_service(service::WM_APP_UPDATE_CHANGED, 0);
    }

    /// 설치 파일 다운로드 후 설치기를 실행하고 프로그램 종료 (설치기가 교체 후 다시 실행함)
    fn install_update(&mut self, info: update::UpdateInfo) {
        {
            let mut st = self.shared.update.lock().unwrap();
            st.progress = Some(0.0);
            st.error = None;
        }
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let result = update::download(&info, |p| {
                shared.update.lock().unwrap().progress = Some(p);
                shared.repaint();
            })
            .and_then(|path| update::run_installer(&path));
            match result {
                Ok(()) => shared.post_service(service::WM_APP_QUIT, 0),
                Err(e) => {
                    let mut st = shared.update.lock().unwrap();
                    st.progress = None;
                    st.error = Some(e);
                    drop(st);
                    shared.repaint();
                }
            }
        });
    }

    fn update_banner(&mut self, ui: &mut egui::Ui) {
        let (info, progress, error) = {
            let st = self.shared.update.lock().unwrap();
            (st.available.clone(), st.progress, st.error.clone())
        };
        let Some(info) = info else { return };

        let mut install = false;
        let mut skip = false;
        egui::Panel::top("update").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.strong(format!("새 버전 v{} 이 있습니다.", info.version_text()));
                ui.weak(format!("(현재 {APP_VERSION})"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let busy = progress.is_some();
                    skip = ui.add_enabled(!busy, egui::Button::new("이 버전 건너뛰기")).clicked();
                    install = ui.add_enabled(!busy, egui::Button::new("지금 설치")).clicked();
                });
            });
            if let Some(p) = progress {
                ui.add(egui::ProgressBar::new(p).show_percentage().text(format!(
                    "다운로드 중... {:.0}%  (완료되면 잠시 종료된 뒤 자동으로 다시 실행됩니다)",
                    p * 100.0
                )));
            }
            if let Some(e) = &error {
                ui.colored_label(ui.visuals().error_fg_color, format!("업데이트 실패: {e}"));
            }
            let notes = info.notes.trim();
            if !notes.is_empty() {
                egui::CollapsingHeader::new("변경 내용").show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                        ui.label(notes);
                    });
                });
            }
            ui.add_space(4.0);
        });

        if install {
            self.install_update(info);
        } else if skip {
            self.skip_update(&info);
        }
    }

    fn toggle_autostart(&mut self) {
        match autostart::set_enabled(self.autostart) {
            Ok(()) => {
                let m = if self.autostart { "Windows 시작 시 자동으로 실행됩니다." } else { "자동 실행을 해제했습니다." };
                self.message = Some(Message::Info(m.into()));
            }
            Err(e) => {
                self.autostart = !self.autostart;
                self.message = Some(Message::Error(format!("자동 실행 설정 실패: {e}")));
            }
        }
    }

    fn hotkey_ui(&mut self, ui: &mut egui::Ui) {
        let status = self.shared.status.lock().unwrap().clone();
        let error = ui.visuals().error_fg_color;
        let keys = &self.keys;
        let d = &mut self.draft;

        egui::Grid::new("hotkey_grid").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
            ui.label("마우스 단축키");
            ui.horizontal(|ui| {
                let enabled = d.mouse.enabled();
                ui.add_enabled(enabled, egui::Checkbox::new(&mut d.mouse.ctrl, "Ctrl"));
                ui.add_enabled(enabled, egui::Checkbox::new(&mut d.mouse.alt, "Alt"));
                ui.add_enabled(enabled, egui::Checkbox::new(&mut d.mouse.shift, "Shift"));
                ui.label("+");
                egui::ComboBox::from_id_salt("mouse_button").selected_text(d.mouse.button.label()).show_ui(ui, |ui| {
                    for b in MouseButton::ALL {
                        ui.selectable_value(&mut d.mouse.button, b, b.label());
                    }
                });
            });
            ui.end_row();

            ui.label("키보드 단축키");
            ui.horizontal(|ui| {
                let enabled = d.keyboard.enabled();
                ui.add_enabled(enabled, egui::Checkbox::new(&mut d.keyboard.ctrl, "Ctrl"));
                ui.add_enabled(enabled, egui::Checkbox::new(&mut d.keyboard.alt, "Alt"));
                ui.add_enabled(enabled, egui::Checkbox::new(&mut d.keyboard.shift, "Shift"));
                ui.add_enabled(enabled, egui::Checkbox::new(&mut d.keyboard.win, "Win"));
                ui.label("+");
                let selected = if enabled { d.keyboard.key.clone() } else { "사용 안 함".to_string() };
                egui::ComboBox::from_id_salt("keyboard_key").selected_text(selected).height(320.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.keyboard.key, String::new(), "사용 안 함");
                    for k in keys {
                        ui.selectable_value(&mut d.keyboard.key, k.clone(), k);
                    }
                });
            })
            .response
            .on_hover_text("마우스 커서 아래의 텍스트를 가져옵니다.");
            ui.end_row();
        });

        if let Some(e) = &status.mouse {
            ui.colored_label(error, format!("마우스 단축키 오류: {e}"));
        }
        if let Some(e) = &status.keyboard {
            ui.colored_label(error, format!("키보드 단축키 오류: {e}"));
        }
        if !d.mouse.enabled() && !d.keyboard.enabled() {
            ui.colored_label(error, "단축키가 모두 꺼져 있어 텍스트를 가져올 수 없습니다.");
        }
    }

    fn text_ui(&mut self, ui: &mut egui::Ui) {
        let d = &mut self.draft;
        ui.horizontal(|ui| {
            ui.label("가져오는 방식");
            egui::ComboBox::from_id_salt("text_method").selected_text(d.text_method.label()).show_ui(ui, |ui| {
                for m in TextMethod::ALL {
                    ui.selectable_value(&mut d.text_method, m, m.label());
                }
            });
        })
        .response
        .on_hover_text(
            "자동: UI Automation 과 MSAA 를 모두 조회해 더 구체적인 결과를 사용합니다.\n특정 프로그램에서 텍스트가 잘 안 잡히면 다른 방식을 시도해 보세요.",
        );
        ui.checkbox(&mut d.auto_copy_selection, "팝업을 열거나 텍스트를 선택하면 자동으로 클립보드에 복사");
        ui.checkbox(&mut d.unicode_spaces_to_ascii, "특수 공백을 일반 공백으로 바꾸고 보이지 않는 문자 제거");
    }

    fn popup_ui(&mut self, ui: &mut egui::Ui) {
        let d = &mut self.draft;
        egui::Grid::new("popup_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("글꼴");
            ui.add(egui::TextEdit::singleline(&mut d.font_name).hint_text("시스템 기본").desired_width(200.0));
            ui.end_row();
            ui.label("글꼴 크기");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut d.font_size).range(0..=72).suffix(" pt"));
                ui.weak("0 이면 시스템 기본");
            });
            ui.end_row();
        });
        ui.checkbox(&mut d.non_resizable, "크기 조절 테두리 숨기기");
    }

    fn buttons_ui(&mut self, ui: &mut egui::Ui) {
        let d = &mut self.draft;
        ui.horizontal(|ui| {
            ui.label("한 줄에 놓을 버튼 수");
            ui.add(egui::DragValue::new(&mut d.buttons_per_row).range(0..=32));
            ui.weak("0 이면 모두 한 줄");
        });
        ui.add_space(4.0);

        let mut action = None;
        if d.web_buttons.is_empty() {
            ui.weak("등록된 버튼이 없습니다.");
        } else {
            let count = d.web_buttons.len();
            egui::Grid::new("buttons_grid").num_columns(6).spacing([6.0, 6.0]).striped(true).show(ui, |ui| {
                ui.strong("이름");
                ui.strong("명령");
                ui.strong("단축");
                ui.strong("창 크기");
                ui.label("");
                ui.end_row();
                // Grid 안에서는 desired_width 가 이전 프레임 열 너비에 묶이므로 크기를 고정
                let h = ui.spacing().interact_size.y;
                for (i, b) in d.web_buttons.iter_mut().enumerate() {
                    ui.add_sized([100.0, h], egui::TextEdit::singleline(&mut b.name));
                    ui.add_sized(
                        [280.0, h],
                        egui::TextEdit::singleline(&mut b.command).hint_text("https://example.com/?q=%s"),
                    );
                    ui.add_sized([28.0, h], egui::TextEdit::singleline(&mut b.key).char_limit(1))
                        .on_hover_text("Alt 와 함께 누를 영문/숫자 (비우면 순번)");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut b.width).range(0..=4000));
                        ui.label("x");
                        ui.add(egui::DragValue::new(&mut b.height).range(0..=4000));
                    })
                    .response
                    .on_hover_text("popup- 명령으로 여는 브라우저 창 크기 (0 이면 기본값)");
                    ui.horizontal(|ui| {
                        if ui.add_enabled(i > 0, egui::Button::new("위")).clicked() {
                            action = Some(ButtonAction::Up(i));
                        }
                        if ui.add_enabled(i + 1 < count, egui::Button::new("아래")).clicked() {
                            action = Some(ButtonAction::Down(i));
                        }
                        if ui.button("삭제").clicked() {
                            action = Some(ButtonAction::Remove(i));
                        }
                    });
                    ui.end_row();
                }
            });
        }
        match action {
            Some(ButtonAction::Up(i)) => d.web_buttons.swap(i, i - 1),
            Some(ButtonAction::Down(i)) => d.web_buttons.swap(i, i + 1),
            Some(ButtonAction::Remove(i)) => {
                d.web_buttons.remove(i);
            }
            None => {}
        }

        ui.horizontal(|ui| {
            if ui.button("+ 버튼 추가").clicked() {
                d.web_buttons.push(WebButton::default());
            }
            if !d.web_buttons.contains(&WebButton::google()) && ui.button("+ 구글 검색").clicked() {
                d.web_buttons.push(WebButton::google());
            }
        });
        egui::CollapsingHeader::new("명령 작성법").show(ui, |ui| {
            ui.label("팝업에서 선택한 텍스트(선택하지 않으면 전체)를 명령에 넣어 실행합니다.");
            ui.label("%s : URL 인코딩한 텍스트      %rs : 원문 그대로");
            ui.label("%ds : 이중 URL 인코딩         %cs : 큰따옴표를 뺀 원문");
            ui.label("popup-web! 를 앞에 붙이면 기본 브라우저(Chrome, Edge, Brave, Firefox)의 작은 앱 창으로 엽니다.");
            ui.label("popup-chrome!, popup-edge!, popup-firefox! 로 브라우저를 직접 지정할 수 있습니다.");
            ui.monospace("popup-web!https://papago.naver.com/?sk=auto&tk=ko&st=%s");
            ui.monospace("notepad.exe");
        });
    }

    fn excluded_ui(&mut self, ui: &mut egui::Ui) {
        ui.weak("이 프로그램 위에서는 마우스 단축키를 가로채지 않습니다 (원격 데스크톱, 게임 등).");
        let mut remove = None;
        for (i, p) in self.draft.excluded_programs.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(p);
                if ui.small_button("삭제").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.draft.excluded_programs.remove(i);
        }
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.new_excluded).hint_text("예: mstsc.exe").desired_width(200.0),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("추가").clicked() || enter) && !self.new_excluded.trim().is_empty() {
                let name = self.new_excluded.trim().to_string();
                if !self.draft.excluded_programs.iter().any(|p| p.eq_ignore_ascii_case(&name)) {
                    self.draft.excluded_programs.push(name);
                }
                self.new_excluded.clear();
            }
        });
    }
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    ui.heading(title);
    ui.add_space(2.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
    ui.add_space(6.0);
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.first_frame {
            self.first_frame = false;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // Ctrl+S 저장
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) && self.is_dirty() {
            self.save_draft();
        }

        self.update_banner(ui);

        let mut autostart_changed = false;
        let mut check_updates = self.draft.check_updates;
        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.horizontal(|ui| {
                autostart_changed = ui.checkbox(&mut self.autostart, "Windows 시작 시 자동 실행").changed();
                ui.checkbox(&mut check_updates, "새 버전 자동 확인").on_hover_text(
                    "시작 직후와 하루 한 번 GitHub 릴리스를 확인합니다.\n트레이 메뉴의 '업데이트 확인'으로 언제든 직접 확인할 수 있습니다.",
                );
                ui.separator();
                ui.weak(format!("{APP_NAME} {APP_VERSION}"));
                ui.weak(" - 창을 닫아도 트레이에서 계속 동작합니다.");
            });
        });
        if autostart_changed {
            self.toggle_autostart();
        }
        if check_updates != self.draft.check_updates {
            self.toggle_check_updates(check_updates);
        }

        let dirty = self.is_dirty();
        let mut do_save = false;
        let mut do_revert = false;
        let mut do_defaults = false;
        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let label = if dirty { "저장 *" } else { "저장" };
                do_save = ui.add_enabled(dirty, egui::Button::new(label)).clicked();
                do_revert = ui.add_enabled(dirty, egui::Button::new("되돌리기")).clicked();
                do_defaults = ui.button("기본값").on_hover_text("기본값을 불러옵니다. 저장해야 적용됩니다.").clicked();
                match &self.message {
                    Some(Message::Info(m)) => {
                        ui.label(m);
                    }
                    Some(Message::Error(m)) => {
                        ui.colored_label(ui.visuals().error_fg_color, m);
                    }
                    None => {}
                }
            });
            ui.add_space(4.0);
        });
        if do_save {
            self.save_draft();
        } else if do_revert {
            self.draft = self.original.clone();
            self.message = None;
        } else if do_defaults {
            let defaults = Config {
                check_updates: self.draft.check_updates,
                skipped_version: self.draft.skipped_version.clone(),
                ..Config::default()
            };
            self.draft = defaults;
            self.message = Some(Message::Info("기본값을 불러왔습니다. 저장하면 적용됩니다.".into()));
        }

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(format!(
                    "{} 를 누르면 커서 아래의 텍스트가 팝업 창에 표시되어 원하는 부분만 선택해 복사할 수 있습니다.",
                    self.original.mouse.display()
                ));
                ui.weak("팝업에서 Tab 키로 이름/값/설명을 차례로 선택하고, Esc 로 닫습니다. 편집하거나 옮기면 창이 고정됩니다.");
                ui.add_space(4.0);
                section(ui, "단축키", |ui| self.hotkey_ui(ui));
                section(ui, "텍스트 가져오기", |ui| self.text_ui(ui));
                section(ui, "팝업 창", |ui| self.popup_ui(ui));
                section(ui, "명령 버튼", |ui| self.buttons_ui(ui));
                section(ui, "제외 프로그램", |ui| self.excluded_ui(ui));
            });
        });
    }
}

impl Drop for SettingsApp {
    fn drop(&mut self) {
        self.shared.ui_hwnd.store(0, Ordering::SeqCst);
        *self.shared.egui_ctx.lock().unwrap() = None;
    }
}
