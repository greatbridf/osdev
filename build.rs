const ARCH_CONFIGS: &[&str] = &["has_stacktrace", "has_shutdown"];

// Define configs

fn define_config(config: &str) {
    println!("cargo::rustc-check-cfg=cfg({config})");
}

fn define_arch_config(config: &str) {
    define_config(&format!("arch_{config}"));
}

fn define_arch_configs() {
    for config in ARCH_CONFIGS {
        define_arch_config(config);
    }
}

fn define_configs() {
    define_arch_configs();
}

// Set configs

fn set_config(config: &str) {
    println!("cargo::rustc-cfg={config}");
}

fn set_arch_config(config: &str) {
    set_config(&format!("arch_{config}"));
}

fn set_arch_configs_x86() {
    set_arch_config("has_stacktrace");
}

fn set_arch_configs_riscv64() {
    set_arch_config("has_stacktrace");
    set_arch_config("has_shutdown");
}

fn set_arch_configs_loongarch64() {
    set_arch_config("has_shutdown");
}

fn set_arch_configs() {
    match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref().unwrap() {
        "x86_64" => set_arch_configs_x86(),
        "riscv64" => set_arch_configs_riscv64(),
        "loongarch64" => set_arch_configs_loongarch64(),
        arch => panic!("Unsupported architecture: {}", arch),
    }
}

fn set_configs() {
    set_arch_configs();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    define_configs();
    set_configs();

    println!("cargo:rustc-link-arg=-T{}", "link.x");

    if let Ok(extra_link_args) = std::env::var("DEP_EONIX_HAL_EXTRA_LINK_ARGS")
    {
        for arg in extra_link_args.split_whitespace() {
            println!("cargo:rustc-link-arg={}", arg);
        }
    }

    Ok(())
}
