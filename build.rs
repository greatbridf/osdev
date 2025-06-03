fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rustc-link-arg=-T{}", "link.x");
    if let Ok(extra_link_args) = std::env::var("DEP_EONIX_HAL_EXTRA_LINK_ARGS") {
        for arg in extra_link_args.split_whitespace() {
            println!("cargo:rustc-link-arg={}", arg);
        }
    }

    Ok(())
}
