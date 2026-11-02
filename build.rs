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
    // Translations: compile each po/<lang>.po into an .mo and list the
    // pairs in a generated manifest for i18n.rs. A missing msgfmt
    // (gettext-tools) degrades to an English-only build with a warning
    // instead of failing.
    let po_dir = Path::new("po");
    let mut langs: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(po_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_po = path.extension().map(|x| x == "po").unwrap_or(false);
            if is_po {
                if let Some(stem) = path.file_stem() {
                    langs.push(stem.to_string_lossy().into_owned());
                }
            }
        }
    }
    langs.sort();
    langs.dedup();
    let have_msgfmt = Command::new("msgfmt")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let mut manifest = String::from("pub static EMBEDDED_LOCALES: &[(&str, &[u8])] = &[\n");
    if !langs.is_empty() && !have_msgfmt {
        println!(
            "cargo:warning=po/*.po found but msgfmt is missing (install gettext); building English-only"
        );
    } else {
        for lang in &langs {
            let mo_dir = out.join("locale").join(lang).join("LC_MESSAGES");
            std::fs::create_dir_all(&mo_dir).unwrap();
            let mo = mo_dir.join("griffel.mo");
            run(Command::new("msgfmt")
                .arg("--check-format")
                .arg("--output-file")
                .arg(&mo)
                .arg(po_dir.join(format!("{lang}.po"))));
            manifest.push_str(&format!(
                "    (\"{lang}\", include_bytes!(concat!(env!(\"OUT_DIR\"), \"/locale/{lang}/LC_MESSAGES/griffel.mo\")) as &[u8]),\n"
            ));
        }
    }
    manifest.push_str("];\n");
    std::fs::write(out.join("locale_manifest.rs"), manifest).unwrap();
    println!("cargo:rerun-if-changed=po");

    println!("cargo:rerun-if-changed=build.rs");
}
