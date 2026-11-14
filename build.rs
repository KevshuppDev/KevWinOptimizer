fn main() {
    // Incrusta el icono en el .exe (usa windres de MinGW)
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("assets/app.rc", embed_resource::NONE);
}
