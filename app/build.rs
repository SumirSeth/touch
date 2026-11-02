fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .set("ProductName", "Touch")
            .compile()
            .expect("embed icon");
    }
}
