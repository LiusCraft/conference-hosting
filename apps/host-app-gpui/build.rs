fn main() {
    println!("cargo:rerun-if-changed=assets/icons/app-taskbar-logo.ico");

    #[cfg(target_os = "windows")]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("assets/icons/app-taskbar-logo.ico");
        resource
            .compile()
            .expect("embed windows icon resource failed");
    }
}
