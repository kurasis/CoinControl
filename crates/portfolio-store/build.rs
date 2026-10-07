fn main() {
    // Stable Rust's SQLx macro watches existing files, not new migration paths.
    println!("cargo:rerun-if-changed=migrations");
}
