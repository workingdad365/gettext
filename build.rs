fn main() {
    println!("cargo:rerun-if-changed=assets/gettext.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        // 트레이 아이콘은 리소스 ID 1 로 로드함
        res.set_icon_with_id("assets/gettext.ico", "1");
        // 팝업 창 버튼 스타일(Common Controls v6)과 모니터별 DPI 인식
        res.set_manifest_file("assets/app.manifest");
        res.compile().expect("Windows 리소스 컴파일 실패");
    }
}
