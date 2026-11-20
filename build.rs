//! На Windows вшиваем иконку в .exe (окно, панель задач, Проводник).

fn main() {
    println!("cargo:rerun-if-changed=assets/windows/app.rc");
    println!("cargo:rerun-if-changed=assets/windows/app.ico");
    #[cfg(windows)]
    embed_resource::compile("assets/windows/app.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("не удалось вшить иконку");
}
