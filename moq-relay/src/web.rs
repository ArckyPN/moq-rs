use std::{net, sync::Arc};

use crate::limiter::*;

use axum::{
	extract::{Path, Query, State},
	http::Method,
	response::IntoResponse,
	routing::{get, post},
	Json, Router,
};
use axum_server::tls_rustls::RustlsAcceptor;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

pub struct WebConfig {
	pub bind: net::SocketAddr,
	pub tls: moq_native::tls::Config,
}

// Run a HTTP server using Axum
// TODO remove this when Chrome adds support for self-signed certificates using WebTransport
pub struct Web {
	app: Router,
	server: axum_server::Server<RustlsAcceptor>,
}

struct Store {
	fingerprint: String,
	limiter: Arc<RwLock<Limiter>>,
}

impl Web {
	pub fn new(config: WebConfig) -> Self {
		// Get the first certificate's fingerprint.
		// TODO serve all of them so we can support multiple signature algorithms.
		let fingerprint = config.tls.fingerprints.first().expect("missing certificate").clone();

		let mut tls = config.tls.server.expect("missing server configuration");
		tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
		let tls = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(tls));

		let store = Arc::new(RwLock::new(Store {
			fingerprint,
			limiter: Arc::new(RwLock::new(Limiter::new(None).unwrap())),
		}));

		let app = Router::new()
			.route("/fingerprint", get(serve_fingerprint))
			.route("/bandwidth/set/:kbps/:latency", post(post_set_bandwidth))
			.route("/bandwidth/remove", post(post_remove_bandwidth))
			.route("/trajectory", post(post_trajectory))
			.layer(
				CorsLayer::new()
					.allow_origin(Any)
					.allow_methods([Method::GET, Method::POST])
					.allow_headers(Any),
			)
			.with_state(store);

		let server = axum_server::bind_rustls(config.bind, tls);

		Self { app, server }
	}

	pub async fn run(self) -> anyhow::Result<()> {
		self.server.serve(self.app.into_make_service()).await?;
		Ok(())
	}
}

async fn serve_fingerprint(State(store): State<Arc<RwLock<Store>>>) -> impl IntoResponse {
	store.read().await.fingerprint.clone()
}

async fn post_set_bandwidth(
	Path((kbps, latency)): Path<(i64, i64)>,
	State(store): State<Arc<RwLock<Store>>>,
) -> impl IntoResponse {
	let limiter = {
		let lock = store.read().await;
		lock.limiter.clone()
	};

	match set_bandwidth(limiter, kbps, latency).await {
		Ok(_) => "ok",
		Err(_) => "failed",
	}
}

async fn post_remove_bandwidth(State(store): State<Arc<RwLock<Store>>>) -> impl IntoResponse {
	let limiter = {
		let lock = store.read().await;
		lock.limiter.clone()
	};

	_ = unset_bandwidth(limiter).await;
	"ok"
}

async fn post_trajectory(
	State(store): State<Arc<RwLock<Store>>>,
	Query(query): Query<TrajectoryQuery>,
	Json(trajectory): Json<Vec<Trajectory>>,
) -> impl IntoResponse {
	let limiter = {
		let lock = store.read().await;
		lock.limiter.clone()
	};

	let l1 = limiter.clone();
	let handle = tokio::spawn(set_trajectory(l1, trajectory, Some(query)));

	let mut lock = limiter.write().await;
	lock.set_handle(handle);

	"ok"
}
