fn main() {
    println!("cargo:rustc-cfg=__warnings");

    #[cfg(all(feature = "cache-repositories", not(feature = "on-disk-cache")))]
    println!("cargo:warning=Feature `cache-repositories` has been renamed to `on-disk-cache`");
}
