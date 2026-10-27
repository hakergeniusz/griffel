use std::path::Path;
use std::process::Command;

fn run(cmd: &mut Command) {
    let status = cmd.status().unwrap_or_else(|e| {
        panic!("failed to spawn {:?}: {e}", cmd.get_program());
    });
    if !status.success() {
        panic!("command failed ({status}): {cmd:?}");
    }
}

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out = Path::new(&out_dir);

    // Compile the GSettings schema so it can be embedded in the GResource.
    run(Command::new("glib-compile-schemas")
        .args(["--targetdir", out_dir.as_str()])
        .arg("resources/schemas"));
    println!("cargo:rerun-if-changed=resources/schemas");

    // Compile the GResource bundle (schemas, styles, tutorial, icons).
    run(Command::new("glib-compile-resources")
        .args(["--sourcedir", "resources", "--sourcedir", out_dir.as_str()])
        .args(["--target", out.join("griffel.gresource").to_str().unwrap()])
        .arg("resources/griffel.gresource.xml"));
    println!("cargo:rerun-if-changed=resources");

    // enchant-2: spell checking (dynamically linked system library).
    let enchant = Command::new("pkg-config")
        .args(["--libs", "--cflags", "enchant-2"])
        .output()
        .expect("pkg-config must be available to locate enchant-2");
    if !enchant.status.success() {
        panic!("enchant-2 development files not found: {}",
            String::from_utf8_lossy(&enchant.stderr));
    }
    for flag in String::from_utf8_lossy(&enchant.stdout).split_whitespace() {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib=dylib={lib}");
        } else if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
