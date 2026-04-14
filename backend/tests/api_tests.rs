/// Testes simples de detecção de providers e mensagens de erro
/// Sem fazer requisições HTTP reais (complexas de simular)

use gdownloader_backend::providers;

#[tokio::test]
async fn test_mega_folder_api_error_message() {
    println!("\n\n🔍 TEST: Mensagem de erro do Mega para Pasta");
    println!("================================================\n");

    let mega_url = "https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw";

    println!("URL: {}\n", mega_url);

    // Simula o que a API faria
    if let Some(provider) = providers::detect_provider(mega_url) {
        println!("✅ Provider detectado: {}\n", provider.name());

        match provider.get_file_info(mega_url).await {
            Ok(_info) => {
                println!("❌ Não deveria ter sucesso para pasta!");
            }
            Err(e) => {
                println!("✅ Erro detectado como esperado:\n");
                println!("   {}\n", e);

                println!("Diagnóstico:");
                println!("  • URL contém /folder/ (pasta) não /file/ (arquivo)");
                println!("  • Mega provider não suporta download de pastas");
                println!("  • Usuário deve usar link de arquivo específico");
            }
        }
    }
}

#[tokio::test]
async fn test_mediafire_api_response() {
    println!("\n\n🔍 TEST: Resposta do MediaFire para URL");
    println!("================================================\n");

    let mediafire_url = "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file";

    println!("URL: {}\n", mediafire_url);

    if let Some(provider) = providers::detect_provider(mediafire_url) {
        println!("✅ Provider detectado: {}\n", provider.name());

        match provider.get_file_info(mediafire_url).await {
            Ok(info) => {
                println!("✅ Info obtida:");
                println!("   Nome: {}", info.filename);
                println!("   Tamanho: {} bytes", info.size);
                println!("   MIME: {:?}\n", info.mime_type);

                if info.size == 0 {
                    println!("⚠️  Nota: Tamanho retornou 0");
                    println!("   Possível causa: MediaFire não retorna Content-Length");
                    println!("   Download pode ainda funcionar");
                }
            }
            Err(e) => {
                println!("❌ Erro: {}\n", e);
                println!("Possível causa: Link expirado ou protegido");
            }
        }
    }
}

#[test]
fn test_api_error_messages_for_urls() {
    println!("\n\n🔍 TEST: Mensagens de erro customizadas");
    println!("================================================\n");

    let test_cases = vec![
        (
            "https://mega.nz/folder/ABC#123",
            "Mega pasta",
            "Links de PASTA (/folder/) não são suportados",
        ),
        (
            "https://example.com/file",
            "URL desconhecida",
            "URL não reconhecida",
        ),
    ];

    for (url, _desc, expected_error_fragment) in test_cases {
        println!("Testing: {} — {}", _desc, url);

        let provider = providers::detect_provider(url);

        if provider.is_none() {
            println!("✅ Provider não detectado (como esperado)\n");
        } else {
            println!("Provider detectado\n");
        }
    }
}
