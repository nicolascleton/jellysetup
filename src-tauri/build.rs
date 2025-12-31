use std::env;
use std::path::Path;

fn main() {
    // Load .env file if it exists
    if Path::new("../.env").exists() {
        dotenv::from_path("../.env").ok();
    } else if Path::new(".env").exists() {
        dotenv::dotenv().ok();
    }

    // Re-run build if .env changes
    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-changed=.env");

    // Pass environment variables to rustc
    if let Ok(url) = env::var("SUPABASE_URL") {
        println!("cargo:rustc-env=SUPABASE_URL={}", url);
    }
    if let Ok(key) = env::var("SUPABASE_ANON_KEY") {
        println!("cargo:rustc-env=SUPABASE_ANON_KEY={}", key);
    }
    if let Ok(service_key) = env::var("SUPABASE_SERVICE_KEY") {
        println!("cargo:rustc-env=SUPABASE_SERVICE_KEY={}", service_key);
    }

    tauri_build::build()
}
