#![cfg(not(target_family = "wasm"))]

use gpui_ce_platform::NativeHttpClient;
use rustls::crypto::CryptoProvider;

#[test]
fn constructing_a_native_client_installs_a_crypto_provider() {
    assert!(CryptoProvider::get_default().is_none());

    NativeHttpClient::new().unwrap();

    assert!(CryptoProvider::get_default().is_some());
}
