use embed_manifest::manifest::ExecutionLevel;
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    embed_manifest(
        new_manifest("GibFonts")
            .requested_execution_level(ExecutionLevel::RequireAdministrator)
            .ui_access(false),
    )
    .expect("cannot embed manifest");
    println!("cargo:rerun-if-changed=build.rs");
}
