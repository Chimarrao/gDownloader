// axum::Json é como res.json() no Express — envolve dados em uma resposta JSON
use axum::Json;
use serde_json::{json, Value};

// Handler HTTP — funções no axum são async e retornam algo que o axum sabe enviar
// O axum detecta automaticamente o tipo de resposta pelo tipo de retorno
pub async fn health() -> Json<Value> {
    // json!() é uma macro que cria um objeto JSON dinamicamente
    // env!("CARGO_PKG_VERSION") lê a versão do Cargo.toml em tempo de compilação
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

// --- Testes unitários ---
// Em Rust, testes ficam no mesmo arquivo dentro de `mod tests`
// #[cfg(test)] significa: "compile isso só quando rodando testes"
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt; // Adiciona .oneshot() para testar handlers sem servidor real
    use http_body_util::BodyExt;

    #[tokio::test] // Marca o teste como assíncrono (necessário porque usamos async/await)
    async fn test_health_returns_ok() {
        // Cria um router só com a rota /health para testar isoladamente
        let app = axum::Router::new()
            .route("/health", axum::routing::get(health));

        // .oneshot() envia uma requisição de teste sem precisar de servidor real
        // É como fazer uma requisição HTTP diretamente para a função handler
        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Lê o body da resposta como bytes e converte para JSON
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }
}
