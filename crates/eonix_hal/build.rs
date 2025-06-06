use std::path::PathBuf;
use std::{env, fs};

fn read_dependent_script(script: &str) -> Result<String, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(script)?;
    println!("cargo:rerun-if-changed={}", script);
    Ok(content)
}

fn process_ldscript_x86(script: &mut String) -> Result<(), Box<dyn std::error::Error>> {
    // Otherwise `bootstrap.rs` might be ignored and not linked in.
    println!("cargo:extra-link-args=--undefined=move_mbr --no-check-sections");

    let memory = read_dependent_script("src/arch/x86_64/memory.x")?;
    let link = read_dependent_script("src/arch/x86_64/link.x")?;

    *script = memory + script;
    script.push_str(&link);

    Ok(())
}

fn process_ldscript_arch(
    script: &mut String,
    arch: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match arch {
        "x86_64" => {
            process_ldscript_x86(script)?;
        }
        _ => panic!("Unsupported architecture: {}", arch),
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let out_script = out_dir.join("link.x");

    let in_script = "src/link.x.in";
    let mut script = read_dependent_script(in_script)?;

    process_ldscript_arch(&mut script, &env::var("CARGO_CFG_TARGET_ARCH")?)?;

    fs::write(out_script, script)?;
    println!("cargo:rustc-link-search={}", out_dir.display());
    Ok(())
}
