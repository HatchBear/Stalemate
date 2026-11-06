fn main() {
    // fluent-dark keeps the few std widgets (scroll bars, combo box) consistent on every platform.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("../../ui/app.slint", config).unwrap();
    println!("cargo:rerun-if-changed=../../ui");
    println!("cargo:rerun-if-changed=../../assets/fonts");
    println!("cargo:rerun-if-changed=../../assets/placeholders");
}
