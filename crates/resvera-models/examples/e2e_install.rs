// End-to-end install check against a real published artifact.
//
// Feeds bytes already fetched from the catalog's download URL through the real
// verification and install path, confirming that what was published actually
// installs and activates.
//
//   curl -fsSL -o /tmp/model.onnx "<catalog download_url>"
//   cargo run -p resvera-models --example e2e_install -- /tmp/model.onnx
//
// The fetch happens outside this crate on purpose: resvera-models is required by
// ADR-0004 to contain no HTTP client, so network access cannot live here.
use resvera_models::{
    load_production_catalog, ModelInstaller, StagedDownloader, RESVERA_PRODUCTION_TRUST_ROOT,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let artifact = args.first().expect("usage: e2e_install <downloaded .onnx>");

    let catalog = load_production_catalog().expect("catalog must verify against the pinned root");
    let entry = &catalog.models[0];
    println!("model:        {}", entry.id);
    println!("expect sha:   {}", entry.sha256);
    println!("expect size:  {}", entry.size_bytes);
    println!("download_url: {:?}", entry.download_urls);

    let bytes = std::fs::read(artifact).expect("read artifact");
    let actual = {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(&bytes))
    };
    println!("actual sha:   {actual}");
    println!("actual size:  {}", bytes.len());

    let tmp = std::env::temp_dir().join("resvera-e2e-install");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let downloader = StagedDownloader::new(&tmp);
    // stage_and_install DOES the install: it verifies the hash, writes the
    // manifest, calls ModelInstaller::install_package, and returns the installed
    // directory. Calling install_package again would fail because nothing is left
    // in staging.
    let installed = downloader
        .stage_and_install(entry, &[&bytes], "", &RESVERA_PRODUCTION_TRUST_ROOT)
        .expect("stage_and_install must accept bytes matching the signed catalog");
    println!("installed:    {}", installed.display());

    let installer = ModelInstaller::new(&tmp);
    let active = installer
        .get_active_version(&entry.id)
        .expect("get_active_version");
    println!("active:       {active:?}");

    if active.as_deref() != Some(entry.version.as_str()) {
        panic!(
            "active version {active:?} != catalog version {}",
            entry.version
        );
    }
    println!("\nOK: published artifact installs and activates through the real code path");
}
