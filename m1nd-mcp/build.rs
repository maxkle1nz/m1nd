fn main() {
    let ui_dist = std::path::Path::new("../m1nd-ui/dist");

    if !ui_dist.exists() {
        println!("cargo:warning=m1nd-ui/dist/ not found. Run: cd m1nd-ui && npm run build");
        std::fs::create_dir_all(ui_dist).ok();
        std::fs::write(
            ui_dist.join("index.html"),
            "<!DOCTYPE html><html><head><title>m1nd</title></head><body><h1>m1nd UI not built</h1><p>Run: cd m1nd-ui && npm run build</p></body></html>",
        ).ok();
    }

    println!("cargo:rerun-if-changed=../m1nd-ui/dist/index.html");
}
