#![cfg_attr(not(test), windows_subsystem = "windows")]

mod accessible;
mod autostart;
mod clipboard;
mod config;
mod launch;
mod mousehook;
mod popup;
mod service;
mod text;
mod ui;
mod update;

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use eframe::egui;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, HWND_MESSAGE, PostMessageW};
use windows::core::w;

use config::Config;

pub enum MainCmd {
    OpenSettings,
}

/// 단축키 등록 실패 사유
#[derive(Debug, Clone, Default)]
pub struct HotkeyStatus {
    pub mouse: Option<String>,
    pub keyboard: Option<String>,
}

/// 스레드 간 공유 상태
pub struct Shared {
    pub config: Mutex<Config>,
    pub status: Mutex<HotkeyStatus>,
    /// 설정 창이 열려 있을 때의 egui 컨텍스트
    pub egui_ctx: Mutex<Option<egui::Context>>,
    /// 설정 창 HWND (닫혀 있으면 0)
    pub ui_hwnd: AtomicIsize,
    pub service_hwnd: AtomicIsize,
    pub update: Mutex<update::UpdateState>,
}

impl Shared {
    pub fn request_reload(&self) {
        self.post_service(service::WM_APP_RELOAD, 0);
    }

    /// 서비스 스레드의 메시지 전용 창에 메시지 전달
    pub fn post_service(&self, msg: u32, wparam: usize) {
        let h = self.service_hwnd.load(Ordering::SeqCst);
        if h != 0 {
            unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut _)), msg, WPARAM(wparam), LPARAM(0));
            }
        }
    }

    /// 설정 창이 열려 있으면 다시 그리도록 요청
    pub fn repaint(&self) {
        if let Some(ctx) = self.egui_ctx.lock().unwrap().as_ref() {
            ctx.request_repaint();
        }
    }
}

/// 이미 실행 중이면 기존 인스턴스에 설정 창 열기를 요청하고 false 반환
fn acquire_single_instance() -> bool {
    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Local\\GetText.SingleInstance"));
        if GetLastError() != ERROR_ALREADY_EXISTS {
            // 핸들은 프로세스 종료 시까지 유지
            std::mem::forget(_mutex);
            return true;
        }
        if let Ok(hwnd) = FindWindowExW(Some(HWND_MESSAGE), None, service::SERVICE_CLASS, None) {
            let _ = PostMessageW(Some(hwnd), service::WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }
        false
    }
}

fn main() {
    if !acquire_single_instance() {
        return;
    }

    autostart::refresh_path();
    let launched_at_login = std::env::args().any(|a| a == autostart::AUTOSTART_ARG);
    let first_run = !Config::path().exists() && !launched_at_login;
    let shared = Arc::new(Shared {
        config: Mutex::new(Config::load()),
        status: Mutex::new(HotkeyStatus::default()),
        egui_ctx: Mutex::new(None),
        ui_hwnd: AtomicIsize::new(0),
        service_hwnd: AtomicIsize::new(0),
        update: Mutex::new(update::UpdateState::default()),
    });

    let (tx, rx) = mpsc::channel();
    service::spawn(shared.clone(), tx.clone());

    // 최초 실행 시에는 사용법을 알 수 있도록 설정 창을 바로 엶
    if first_run {
        let _ = tx.send(MainCmd::OpenSettings);
    }

    // 설정 창은 요청이 있을 때만 메인 스레드에서 생성하고, 닫히면 다음 요청을 기다림
    while let Ok(MainCmd::OpenSettings) = rx.recv() {
        ui::run_settings(shared.clone());
        shared.ui_hwnd.store(0, Ordering::SeqCst);
        // 창이 열려 있는 동안 쌓인 중복 요청 제거
        while rx.try_recv().is_ok() {}
    }
}
