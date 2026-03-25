fn main() {
    // Tell cargo to re-run if any demo-ui file changes (used by include_str!).
    println!("cargo::rerun-if-changed=../../demo-ui/");
}
