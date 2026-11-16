fn main() {
    // Incrusta el icono en el .exe (usa windres de MinGW)
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=Cargo.toml");
    embed_resource::compile("assets/app.rc", embed_resource::NONE);
}
