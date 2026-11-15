//! Embeds the application icon into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon/kubernet-wizard.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon/kubernet-wizard.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows icon: {e}");
        }
    }
}
