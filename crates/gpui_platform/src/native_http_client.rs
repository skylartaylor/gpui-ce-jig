use futures::future::BoxFuture;
use gpui::http_client::{HttpClient, HttpResponse};
use reqwest::{blocking::Client, redirect::Policy};
use std::time::Duration;

/// An opt-in native HTTP client for applications that load remote resources.
///
/// Constructing this client does not install it globally. Pass it to
/// [`gpui::Application::with_http_client`] when network access is appropriate
/// for the application.
pub struct NativeHttpClient {
    follow_redirects: Client,
    reject_redirects: Client,
}

impl NativeHttpClient {
    /// Builds a client with GPUI's default user agent.
    pub fn new() -> anyhow::Result<Self> {
        Self::user_agent(concat!("gpui-ce/", env!("CARGO_PKG_VERSION")))
    }

    /// Builds a client with the provided HTTP user agent.
    pub fn user_agent(user_agent: impl AsRef<str>) -> anyhow::Result<Self> {
        let user_agent = user_agent.as_ref();
        Ok(Self {
            follow_redirects: build_client(user_agent, Policy::limited(10))?,
            reject_redirects: build_client(user_agent, Policy::none())?,
        })
    }
}

fn build_client(user_agent: &str, redirect: Policy) -> anyhow::Result<Client> {
    Ok(Client::builder()
        .user_agent(user_agent)
        .redirect(redirect)
        .connect_timeout(Duration::from_secs(10))
        .timeout(None)
        .build()?)
}

impl HttpClient for NativeHttpClient {
    fn get(
        &self,
        url: &str,
        follow_redirects: bool,
    ) -> BoxFuture<'static, anyhow::Result<HttpResponse>> {
        let client = if follow_redirects {
            self.follow_redirects.clone()
        } else {
            self.reject_redirects.clone()
        };
        let url = url.to_owned();
        Box::pin(async move {
            smol::unblock(move || {
                let response = client.get(url).send()?;
                let status = response.status();
                let body = response.bytes()?.to_vec();
                Ok(HttpResponse { status, body })
            })
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read as _, Write as _},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread,
    };

    fn read_request(stream: &mut TcpStream) -> std::io::Result<()> {
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte)?;
            request.push(byte[0]);
        }
        Ok(())
    }

    fn respond(stream: &mut TcpStream, status: &str, headers: &str, body: &[u8]) {
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        stream.flush().unwrap();
    }

    fn one_response(status: &'static str, body: &'static [u8]) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream).unwrap();
            respond(&mut stream, status, "Connection: close\r\n", body);
        });
        (format!("http://{address}"), server)
    }

    fn fetch(
        client: &NativeHttpClient,
        url: &str,
        follow_redirects: bool,
    ) -> anyhow::Result<HttpResponse> {
        smol::block_on(client.get(url, follow_redirects))
    }

    #[test]
    fn returns_response_status_and_body() {
        let (url, server) = one_response("200 OK", b"image");
        let response = fetch(&NativeHttpClient::new().unwrap(), &url, true).unwrap();
        server.join().unwrap();

        assert_eq!(response.status, reqwest::StatusCode::OK);
        assert_eq!(response.body, b"image");
    }

    #[test]
    fn preserves_non_success_responses() {
        let (url, server) = one_response("404 Not Found", b"missing");
        let response = fetch(&NativeHttpClient::new().unwrap(), &url, true).unwrap();
        server.join().unwrap();

        assert_eq!(response.status, reqwest::StatusCode::NOT_FOUND);
        assert_eq!(response.body, b"missing");
    }

    #[test]
    fn honors_redirect_mode() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            read_request(&mut first).unwrap();
            respond(
                &mut first,
                "302 Found",
                "Location: /final\r\nConnection: close\r\n",
                b"redirect",
            );
            let (mut second, _) = listener.accept().unwrap();
            read_request(&mut second).unwrap();
            respond(&mut second, "200 OK", "Connection: close\r\n", b"final");
        });
        let client = NativeHttpClient::new().unwrap();
        let response = fetch(&client, &format!("http://{address}"), true).unwrap();
        server.join().unwrap();
        assert_eq!(response.status, reqwest::StatusCode::OK);
        assert_eq!(response.body, b"final");

        let (url, server) = one_response("302 Found", b"redirect");
        let response = fetch(&client, &url, false).unwrap();
        server.join().unwrap();
        assert_eq!(response.status, reqwest::StatusCode::FOUND);
        assert_eq!(response.body, b"redirect");
    }

    #[test]
    fn reports_invalid_and_transport_errors() {
        let client = NativeHttpClient::new().unwrap();
        assert!(fetch(&client, "not a URL", true).is_err());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        assert!(fetch(&client, &format!("http://{address}"), true).is_err());
    }

    #[test]
    fn reuses_the_clients_built_at_construction() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (connections_tx, connections_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            read_request(&mut first).unwrap();
            respond(&mut first, "200 OK", "Connection: keep-alive\r\n", b"one");
            first
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let reused_connection = if read_request(&mut first).is_ok() {
                respond(&mut first, "200 OK", "Connection: close\r\n", b"two");
                true
            } else {
                false
            };
            connections_tx.send(reused_connection).unwrap();
        });

        let client = NativeHttpClient::user_agent("gpui-test").unwrap();
        assert_eq!(
            fetch(&client, &format!("http://{address}/one"), true)
                .unwrap()
                .body,
            b"one"
        );
        assert_eq!(
            fetch(&client, &format!("http://{address}/two"), true)
                .unwrap()
                .body,
            b"two"
        );
        server.join().unwrap();
        assert!(connections_rx.recv().unwrap());
    }
}
