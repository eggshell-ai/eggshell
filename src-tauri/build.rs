fn main() {
    // `modules.rs` embeds the whole module tree with `include_dir!`, but Cargo
    // only tracks files a proc macro *declares* through `include_str!`-style
    // expansion. Without this line, editing a module (e.g. fixing a PHP file)
    // is invisible to the build and the old embedded copy keeps being tested
    // and shipped. Cargo scans a directory path recursively.
    println!("cargo:rerun-if-changed=../modules");

    tauri_build::build()
}
