fn main() {
    cc::Build::new()
        .file("./libsecp256k1/test.c")
        .compile("test.a");
}
