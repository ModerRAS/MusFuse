fn main() {
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        winfsp::build::winfsp_link_delayload();
    }
}
