fn main() {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    println!("cargo:rustc-link-arg=-Tlinker-{arch}.ld");
    println!("cargo:rerun-if-changed=linker-{arch}.ld");
    if arch == "x86_64" {
        cc::Build::new().file("src/idt.S").compile("idt");
        println!("cargo:rerun-if-changed=src/idt.S");
    }
}
