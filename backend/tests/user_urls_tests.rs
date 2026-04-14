/// Testes de integração com as URLs do usuário
/// Estes testes verificam se as URLs funcionam corretamente no backend
///
/// Para rodar: cargo test --test user_urls_tests -- --nocapture
///
/// URLs sendo testadas:
/// 1. https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw
/// 2. https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file

use gdownloader_backend::providers;

#[tokio::test]
async fn test_user_mega_url() {
    println!("\n\n🔍 TEST: Mega Folder URL (do usuário)");
    println!("================================================\n");

    let url = "https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw";

    println!("URL: {}\n", url);

    // Step 1: Provider detection
    println!("Step 1: Provider Detection");
    let provider = providers::detect_provider(url);

    match provider {
        Some(p) => {
            println!("✅ Provider detectado: {}\n", p.name());

            // Step 2: Try to get file info
            println!("Step 2: Obtendo informações do arquivo...");
            match p.get_file_info(url).await {
                Ok(info) => {
                    println!("✅ SUCESSO!");
                    println!("   Nome: {}", info.filename);
                    println!("   Tamanho: {} bytes", info.size);
                    println!("   MIME: {:?}", info.mime_type);
                }
                Err(e) => {
                    println!("❌ ERRO: {}", e);
                    println!("\n🔎 DIAGNÓSTICO:");

                    let error_msg = e.to_string();
                    if error_msg.contains("inválida") || error_msg.contains("invalid") {
                        println!("   A URL não é válida para o provedor Mega.");
                        println!("   Links de PASTA (/folder/) não são suportados.");
                        println!("   Use um link de ARQUIVO (/file/) em vez disso.");
                    } else if error_msg.contains("não encontrado") || error_msg.contains("not found") {
                        println!("   O arquivo/pasta não foi encontrado.");
                        println!("   O link pode estar expirado ou deletado.");
                    } else if error_msg.contains("acesso negado") || error_msg.contains("access denied") {
                        println!("   Acesso negado ao arquivo.");
                        println!("   Pode ser privado ou requerer autenticação.");
                    }
                }
            }
        }
        None => {
            println!("❌ Provider NÃO foi detectado para esta URL\n");
            println!("A URL não corresponde a nenhum provedor conhecido.");
        }
    }
}

#[tokio::test]
async fn test_user_mediafire_url() {
    println!("\n\n🔍 TEST: MediaFire File URL (do usuário)");
    println!("================================================\n");

    let url = "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file";
    let url_cleaned = "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv";

    println!("URL Original: {}\n", url);
    println!("URL Limpa:    {}\n", url_cleaned);

    // Test both versions
    test_mediafire_variant("Original (com /file duplicado)", url).await;
    println!("\n---\n");
    test_mediafire_variant("Limpa (sem /file no final)", url_cleaned).await;
}

async fn test_mediafire_variant(variant_name: &str, url: &str) {
    println!("Variante: {}\n", variant_name);

    // Step 1: Provider detection
    println!("Step 1: Provider Detection");
    let provider = providers::detect_provider(url);

    match provider {
        Some(p) => {
            println!("✅ Provider detectado: {}\n", p.name());

            // Step 2: Try to get file info
            println!("Step 2: Obtendo informações do arquivo...");
            match p.get_file_info(url).await {
                Ok(info) => {
                    println!("✅ SUCESSO!");
                    println!("   Nome: {}", info.filename);
                    println!("   Tamanho: {} bytes", info.size);

                    if info.size == 0 {
                        println!("\n⚠️  AVISO: Tamanho retornou 0");
                        println!("   Possíveis causas:");
                        println!("   • MediaFire pode estar bloqueando requisições HEAD");
                        println!("   • O servidor pode estar usando Transfer-Encoding: chunked");
                        println!("   • O link pode estar protegido ou expirado");
                        println!("   • O arquivo pode estar temporariamente indisponível");
                    }

                    println!("   MIME: {:?}", info.mime_type);
                }
                Err(e) => {
                    println!("❌ ERRO: {}", e);
                    println!("\n🔎 DIAGNÓSTICO:");

                    let error_msg = e.to_string();
                    if error_msg.contains("não encontrado") || error_msg.contains("not found") {
                        println!("   Link não encontrado na página.");
                        println!("   A página pode estar quebrada ou o link inválido.");
                    } else if error_msg.contains("Link de download não encontrado") {
                        println!("   O scraper não conseguiu extrair o link de download.");
                        println!("   MediaFire pode ter alterado sua página HTML.");
                        println!("   Ou o link pode estar protegido por captcha.");
                    }
                }
            }
        }
        None => {
            println!("❌ Provider NÃO foi detectado para esta URL\n");
        }
    }
}

#[test]
fn test_supported_formats_mega() {
    println!("\n\n📋 TEST: Formatos suportados do Mega");
    println!("================================================\n");

    let test_cases = vec![
        ("https://mega.nz/file/ABC123#key456", true, "Arquivo moderno /file/"),
        ("https://mega.nz/#!ABC123!key456", true, "Arquivo antigo /#!"),
        ("https://mega.nz/folder/ABC123#key456", false, "Pasta /folder/ (NÃO SUPORTADA)"),
        ("https://mega.nz/#ARQUIVO#key456", false, "Formato inválido"),
    ];

    for (url, expected_support, description) in test_cases {
        let provider = providers::detect_provider(url);
        let detected = provider.is_some();

        let status = if detected == expected_support {
            "✅"
        } else {
            "❌"
        };

        println!("{} {} — {}", status, description, url);
    }
}

#[test]
fn test_supported_formats_mediafire() {
    println!("\n\n📋 TEST: Formatos suportados do MediaFire");
    println!("================================================\n");

    let test_cases = vec![
        ("https://www.mediafire.com/file/abc123/arquivo.zip", true, "Padrão /file/"),
        ("https://mediafire.com/file/abc123/arquivo.zip", true, "Sem www"),
        ("https://www.mediafire.com/file/abc123/arquivo.zip/file", true, "Com /file duplicado (não ideal)"),
        ("https://www.mediafire.com/download/abc123/arquivo.zip", true, "Com /download/"),
        ("https://example.com/file/abc123", false, "Domínio diferente"),
    ];

    for (url, expected_support, description) in test_cases {
        let provider = providers::detect_provider(url);
        let detected = provider.is_some();

        let status = if detected == expected_support {
            "✅"
        } else {
            "❌"
        };

        println!("{} {} — {}", status, description, url);
    }
}

#[test]
fn test_url_validation() {
    println!("\n\n🔐 TEST: Validação de URLs");
    println!("================================================\n");

    println!("Testando URLs do usuário:\n");

    let mega_url = "https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw";
    let mediafire_url = "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file";

    println!("1. Mega URL: {}", mega_url);
    println!("   Status: {}", if mega_url.contains("mega.nz") { "✅ Domínio reconhecido" } else { "❌ Domínio não reconhecido" });
    println!("   Tipo: {}", if mega_url.contains("/file/") { "📄 Arquivo" } else if mega_url.contains("/folder/") { "📁 Pasta" } else { "❓ Desconhecido" });
    println!("   Hash: {}", if mega_url.contains('#') { "✅ Presente" } else { "❌ Ausente" });

    println!("\n2. MediaFire URL: {}", mediafire_url);
    println!("   Status: {}", if mediafire_url.contains("mediafire.com") { "✅ Domínio reconhecido" } else { "❌ Domínio não reconhecido" });
    println!("   Tipo: {}", if mediafire_url.contains("/file/") { "📄 Arquivo" } else if mediafire_url.contains("/folder/") { "📁 Pasta" } else { "❓ Desconhecido" });
    println!("   Problema: {}", if mediafire_url.ends_with("/file") { "⚠️  /file no final (provável erro)" } else { "✅ Sem problemas óbvios" });
}

#[tokio::test]
async fn test_error_messages() {
    println!("\n\n📢 TEST: Mensagens de erro (Debug)");
    println!("================================================\n");

    let mega_provider = providers::detect_provider("https://mega.nz/something").unwrap();

    // Force an error by using invalid URL format
    match mega_provider.get_file_info("https://mega.nz/folder/ABC#123").await {
        Ok(_) => println!("❌ Esperava erro, mas funcionou"),
        Err(e) => println!("✅ Erro capturado: {}", e),
    }
}
