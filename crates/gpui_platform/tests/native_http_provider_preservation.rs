#![cfg(not(target_family = "wasm"))]

use gpui_ce_platform::NativeHttpClient;
use rustls::crypto::{CryptoProvider, ring};

#[test]
fn constructing_a_native_client_preserves_an_installed_crypto_provider() {
    let mut sentinel = ring::default_provider();
    sentinel.cipher_suites.truncate(1);
    sentinel.install_default().unwrap();

    NativeHttpClient::new().unwrap();

    assert_eq!(
        CryptoProvider::get_default().unwrap().cipher_suites.len(),
        1
    );
}
