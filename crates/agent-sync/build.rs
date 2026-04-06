fn main() {
    let version = std::env::var("AGENT_TOOLS_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=AGENT_TOOLS_VERSION={version}");
}
