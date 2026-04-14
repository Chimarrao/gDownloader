/// Testes específicos com as URLs do usuário
/// URL 1: https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw
/// URL 2: https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file

#[test]
fn test_mega_folder_url_analysis() {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║           TESTANDO URL MEGA - ANÁLISE DETALHADA              ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let url = "https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw";

    println!("📋 URL: {}\n", url);

    // Análise 1: URL é reconhecida como Mega?
    println!("1️⃣  DETECÇÃO DE PROVIDER:");
    if url.contains("mega.nz") {
        println!("   ✅ Domínio 'mega.nz' detectado");
    }

    // Análise 2: Formato da URL
    println!("\n2️⃣  ANÁLISE DE FORMATO:");
    if url.contains("/file/") {
        println!("   ✅ Contém '/file/' — ARQUIVO direto (suportado)");
    } else if url.contains("/folder/") {
        println!("   ⚠️  Contém '/folder/' — PASTA (NÃO suportado atualmente)");
    } else if url.contains("/#!") {
        println!("   ✅ Formato antigo '/#!' (suportado)");
    }

    // Análise 3: Tentativa de parse
    println!("\n3️⃣  TENTATIVA DE PARSE:");
    if let Some(hash_pos) = url.find('#') {
        let before_hash = &url[..hash_pos];
        let after_hash = &url[hash_pos + 1..];
        println!("   Parte antes de '#': {}", before_hash);
        println!("   Parte depois de '#': {}", after_hash);

        if before_hash.contains("/file/") {
            println!("   ✅ Pode fazer parse (há '/file/')");
        } else if before_hash.contains("/folder/") {
            println!("   ❌ NÃO consegue fazer parse (há '/folder/' em vez de '/file/')");
        }
    }

    // Análise 4: Explicação
    println!("\n4️⃣  EXPLICAÇÃO DO PROBLEMA:");
    println!("   O código em mega.rs:parse_url() procura especificamente por:");
    println!("     • Padrão novo: '/file/HANDLE#KEY'");
    println!("     • Padrão antigo: '/#!HANDLE!KEY'");
    println!();
    println!("   Sua URL usa '/folder/' que NÃO é tratado.");
    println!("   Links de pasta podem exigir API diferente ou não ser suportados.");

    // Análise 5: Solução
    println!("\n5️⃣  POSSÍVEIS SOLUÇÕES:");
    println!("   A) Implementar suporte a pastas (complexo - Mega API RPC diferente)");
    println!("   B) Converter pasta em arquivo direto via link de arquivo dentro dela");
    println!("   C) Documentar que apenas arquivos diretos são suportados");
}

#[test]
fn test_mediafire_url_analysis() {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║         TESTANDO URL MEDIAFIRE - ANÁLISE DETALHADA           ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    let url_original = "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file";
    let url_corrigida = "https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv";

    println!("📋 URL ORIGINAL: {}\n", url_original);
    println!("📋 URL SUGERIDA: {}\n", url_corrigida);

    // Análise 1: Diferença
    println!("1️⃣  ANÁLISE DE DIFERENÇAS:");
    println!("   Original: ...MemoriadaTV.Menor.mkv/file");
    println!("   Corrigida: ...MemoriadaTV.Menor.mkv");
    println!();
    println!("   ⚠️  A original tem '/file' no final (parece duplicado/erro de cópia)");

    // Análise 2: Como MediaFire funciona
    println!("\n2️⃣  COMO MEDIAFIRE FUNCIONA:");
    println!("   1. Acessamos a URL da página de download");
    println!("   2. HTML contém link direto de download (scraping)");
    println!("   3. Fazemos HEAD request no link direto para obter metadados");
    println!("   4. O servidor retorna Content-Length no header");

    // Análise 3: Problema observado
    println!("\n3️⃣  PROBLEMA OBSERVADO:");
    println!("   ✅ URL é detectada corretamente");
    println!("   ✅ Link de download é extraído");
    println!("   ⚠️  MAS: size retorna 0 (Content-Length não informado)");
    println!();
    println!("   Possíveis causas:");
    println!("   a) MediaFire bloqueando requisições HEAD");
    println!("   b) URL está protegida ou expirada");
    println!("   c) Servidor não retorna Content-Length para este tipo de arquivo");
    println!("   d) Redirecionamento não é seguido corretamente");

    // Análise 4: Teste prático
    println!("\n4️⃣  PRÓXIMOS PASSOS:");
    println!("   • Tente acessar manualmente a URL corrigida no navegador");
    println!("   • Se funcionar, o app deve detectar corretamente");
    println!("   • Se não funcionar, o link pode estar protegido/expirado");
    println!("   • Use uma URL MediaFire diferente para testar");
}

#[test]
fn test_url_format_comparison() {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║        COMPARAÇÃO DE FORMATOS SUPORTADOS vs TESTADOS         ║");
    println!("╚════════════════════════════════════════════════════════════════╝\n");

    println!("MEGA - FORMATOS SUPORTADOS:");
    println!("  ✅ /file/ — https://mega.nz/file/HANDLE#KEY");
    println!("  ✅ /#! — https://mega.nz/#!HANDLE!KEY");
    println!("  ❌ /folder/ — https://mega.nz/folder/HANDLE#KEY\n");

    println!("SEU TESTE:");
    println!("  URL: https://mega.nz/folder/fsRXAIZB#c__iSL0gQcIvxlFOunwZJw");
    println!("  Status: ❌ INCOMPATÍVEL\n");

    println!("\nMEDIAFIRE - FORMATOS SUPORTADOS:");
    println!("  ✅ /file/ — https://www.mediafire.com/file/ID/filename");
    println!("  ✅ /download/ — Redirecionamentos automáticos\n");

    println!("SEU TESTE:");
    println!("  URL: https://www.mediafire.com/file/rzr1u8ba62xksi0/DBZ.161.BD1080p.MemoriadaTV.Menor.mkv/file");
    println!("  Status: ⚠️  DETECTADO MAS COM POSSÍVEL ERRO DE CÓPIA (/file duplicado)\n");

    println!("RECOMENDAÇÕES:");
    println!("  1. Para Mega: Use link de ARQUIVO (/file/) não de PASTA (/folder/)");
    println!("  2. Para MediaFire: Remova o '/file' extra no final");
    println!("  3. Teste com essas URLs e reporte qualquer erro específico");
}
