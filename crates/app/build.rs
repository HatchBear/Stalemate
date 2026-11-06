fn main() {
    // fluent-dark keeps the few std widgets (scroll bars, combo box) consistent on every platform.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("../../ui/app.slint", config).unwrap();
    // The git commit, for the diagnostics file's about.json. "unknown" outside a git checkout.
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=STALEMATE_COMMIT={commit}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../ui");
    println!("cargo:rerun-if-changed=../../assets/fonts");
    println!("cargo:rerun-if-changed=../../assets/placeholders");
}
