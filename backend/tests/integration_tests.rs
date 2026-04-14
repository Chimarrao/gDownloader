// Testes de integração com URLs reais fornecidas pelo usuário
use gdownloader_backend::ws;

/// TESTES COM AS URLs DO USUÁRIO
///
/// URL 1: https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw
/// URL 2: https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file

#[tokio::test]
async fn test_mega_folder_url_detection() {
    let url = "https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw";

    // Teste 1: Provider detecta a URL?
    let provider = gdownloader_backend::providers::detect_provider(url);
    println!("Mega folder URL detectada? {:?}", provider.is_some());

    // Teste 2: Se detectou, consegue fazer parse?
    if let Some(provider) = provider {
        println!("Provider detectado: {}", provider.name());

        // Tenta obter info do arquivo
        match provider.get_file_info(url).await {
            Ok(info) => {
                println!("✅ Info obtida: {} ({})", info.filename, info.size);
            }
            Err(e) => {
                println!("❌ Erro ao obter info: {}", e);
                println!("   Motivo provável: URL é de PASTA, código só suporta ARQUIVOS");
            }
        }
    } else {
        println!("❌ Provider não detectou a URL");
    }
}

#[tokio::test]
async fn test_mediafire_url_with_duplicate_file() {
    let url_com_erro = "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file";
    let url_corrigida = "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv";

    // Teste 1: URL com duplicação de /file
    println!("\n--- Testando URL com erro ---");
    let provider = gdownloader_backend::providers::detect_provider(url_com_erro);
    println!("URL com /file duplicado detectada? {:?}", provider.is_some());

    if let Some(provider) = provider {
        println!("Provider: {}", provider.name());
        match provider.get_file_info(url_com_erro).await {
            Ok(info) => {
                println!("✅ Info obtida: {} ({})", info.filename, info.size);
            }
            Err(e) => {
                println!("❌ Erro: {}", e);
            }
        }
    }

    // Teste 2: URL corrigida
    println!("\n--- Testando URL corrigida ---");
    let provider = gdownloader_backend::providers::detect_provider(url_corrigida);
    if let Some(provider) = provider {
        println!("Provider: {}", provider.name());
        match provider.get_file_info(url_corrigida).await {
            Ok(info) => {
                println!("✅ Info obtida: {} ({})", info.filename, info.size);
            }
            Err(e) => {
                println!("❌ Erro: {}", e);
            }
        }
    }
}

#[tokio::test]
async fn test_mega_parse_formats() {
    // Testa diferentes formatos de URL do Mega

    println!("\n=== Testando diferentes formatos Mega ===\n");

    // Formato de ARQUIVO (suportado)
    let arquivo = "https://mega.nz/file/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw";
    println!("ARQUIVO: {}", arquivo);
    if gdownloader_backend::providers::detect_provider(arquivo).is_some() {
        println!("✅ Formato detectado\n");
    } else {
        println!("❌ Formato NÃO detectado\n");
    }

    // Formato de PASTA (NÃO suportado atualmente)
    let pasta = "https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw";
    println!("PASTA: {}", pasta);
    if gdownloader_backend::providers::detect_provider(pasta).is_some() {
        println!("⚠️  Formato detectado MAS...");
        let provider = gdownloader_backend::providers::detect_provider(pasta).unwrap();
        println!("   Provider: {}", provider.name());
        println!("   ❌ Parsing de pasta pode falhar (código não suporta)");
    } else {
        println!("❌ Formato NÃO detectado\n");
    }

    // Formato antigo (suportado)
    let antigo = "https://mega.nz/#!fsRXAIZB!c__iSL0gQcIvxlFOunwZJw";
    println!("ANTIGO: {}", antigo);
    if gdownloader_backend::providers::detect_provider(antigo).is_some() {
        println!("✅ Formato detectado\n");
    } else {
        println!("❌ Formato NÃO detectado\n");
    }
}

#[test]
fn test_provider_detection_logic() {
    println!("\n=== Testando lógica de detecção ===\n");

    // Mega com /file/
    let url1 = "https://mega.nz/file/AbCdEfGh#key123";
    println!("URL: {}", url1);
    println!("matches(mega): {}", url1.contains("mega.nz"));
    println!("parse_url retorna: {}\n",
        if url1.find("/file/").is_some() { "Some" } else { "None" });

    // Mega com /folder/
    let url2 = "https://mega.nz/folder/AbCdEfGh#key123";
    println!("URL: {}", url2);
    println!("matches(mega): {}", url2.contains("mega.nz"));
    println!("parse_url retorna: {}\n",
        if url2.find("/file/").is_some() || url2.find("#!").is_some() { "Some" } else { "None" });
    println!("❌ PROBLEMA: /folder/ não é tratado!\n");

    // MediaFire com /file duplicado
    let url3 = "https://www.mediafire.com/file/rzr1u8ba62xksi0/file.mkv/file";
    println!("URL: {}", url3);
    println!("matches(mediafire): {}", url3.contains("mediafire.com"));
    println!("Último /file pode confundir o scraper\n");
}

#[tokio::test]
async fn test_router_with_invalid_urls() {
    println!("\n=== Testando rotas com URLs inválidas ===\n");

    let state = ws::AppState::new();
    let router = gdownloader_backend::create_router(state);

    let client = reqwest::Client::new();
    let url_base = "http://localhost:5000"; // Isto seria a porta real em um teste real

    // Este teste é conceitual - na prática precisaríamos rodar o servidor
    // Mas aqui testamos a lógica de provider detection

    let urls = vec![
        "https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw",
        "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.mkv/file",
        "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.mkv",
    ];

    for url in urls {
        println!("Testando: {}", url);
        let provider = gdownloader_backend::providers::detect_provider(url);
        println!("Resultado: {}\n",
            if provider.is_some() { "✅ Detectada" } else { "❌ Não detectada" });
    }
}
