// Ícones de aplicativo por formato de arquivo, para quando o provedor não tem um
// SVG de marca (ex.: download HTTP direto). MKV → VLC, RAR → WinRAR, etc.
//
// Observação: os SVGs coloridos do icons8 são um formato PAGO (a API responde
// PAID_FORMAT), então não dá para baixá-los em massa. Para adicionar/atualizar um
// ícone, copie o SVG no icons8 (botão "copiar") e cole em APP_ICONS abaixo, depois
// mapeie as extensões em EXT_TO_APP. O do VLC veio direto do icons8.

export interface FileTypeIcon {
  app: string
  svg: string
}

// Cada chave é um "app"/família de ícone; o valor é o SVG colorido (48x48).
const APP_ICONS: Record<string, string> = {
  // VLC (vídeo) — SVG oficial do icons8.
  vlc: `<svg xmlns="http://www.w3.org/2000/svg" x="0px" y="0px" width="100" height="100" viewBox="0 0 48 48"><path fill="#F57C00" d="M36.258,28.837c0,0-0.11-0.837-1.257-0.837c-0.216,0-2.392,0-3.719,0c0.798,2.671,1.497,5.135,1.497,5.279c0,2.387-3.401,3.393-8.917,3.393c-5.515,0-8.651-0.94-8.651-3.326c0-0.167,0.998-2.692,1.791-5.346c-1.591,0-3.863,0-4.063,0c-0.806,0-0.937,0.749-0.937,0.749L8.159,40.986L8.815,42h30.652l0.376-1.014L36.258,28.837z"></path><path fill="#E0E0E0" d="M24.001,6c-1.029,0-1.864,0.179-1.864,0.398c-0.492,1.483-8.122,26.143-8.122,26.774c0,2.388,4.471,3.827,9.985,3.827s9.986-1.439,9.986-3.827c0-0.549-7.614-25.268-8.122-26.774C25.865,6.179,25.031,6,24.001,6L24.001,6z"></path><path fill="#FF9800" d="M33.196 30.447C32.032 32.232 28.341 34 24.046 34c-4.34 0-8.156-1.696-9.281-3.51-.499 1.483-.892 2.647-.892 3.28 0 2.386 4.533 4.229 10.128 4.229 5.595 0 10.131-1.844 10.131-4.229C34.132 33.222 33.713 31.955 33.196 30.447zM31.387 24.314l-2.074-6.794c0 0-1.857 1.479-5.311 1.479-3.453 0-5.316-1.479-5.316-1.479l-2.081 6.806c0 0 2.068 2.674 7.397 2.674C29.375 27 31.387 24.314 31.387 24.314zM27.241 10.809l-1.376-4.41c0 0-.083-.398-1.864-.398-1.844 0-1.864.398-1.864.398l-1.376 4.407c0 0 .885 1.194 3.239 1.194C26.355 12 27.241 10.809 27.241 10.809z"></path></svg>`,

  // WinRAR (.rar) — ícone oficial do icons8 (fornecido/licenciado pelo usuário).
  winrar: `<svg xmlns="http://www.w3.org/2000/svg" x="0px" y="0px" width="100" height="100" viewBox="0 0 48 48"><path fill="#CFD8DC" d="M17 24L7 17.467c0 0 1-1.865 1-3.732S7 10 7 10l10 6.533V24zM17 40L7 32.533c0 0 1-.934 1-2.8S7 26 7 26l10 6.533V40zM17 32L7 24.533c0 0 1-.934 1-2.8S7 18 7 18l10 6.533V32z"></path><path fill="#3F51B5" d="M17.624,34c-0.219,0-0.44-0.071-0.624-0.219l-10.625-8c-0.432-0.346-0.501-0.975-0.156-1.406c0.344-0.431,0.975-0.501,1.405-0.156l10.625,8c0.432,0.346,0.501,0.975,0.156,1.406C18.208,33.871,17.917,34,17.624,34z"></path><path fill="#3F51B5" d="M42,25H16c0,0,1,1.742,1,4.246S16,33,16,33h26c0,0,1-1,1-4S42,25,42,25z"></path><path fill="#9C27B0" d="M17.624,26c-0.219,0-0.44-0.071-0.624-0.219l-10.625-8c-0.432-0.346-0.501-0.975-0.156-1.406c0.344-0.431,0.975-0.502,1.405-0.156l10.625,8c0.432,0.346,0.501,0.975,0.156,1.406C18.208,25.871,17.917,26,17.624,26z"></path><path fill="#9C27B0" d="M42,17H16c0,0,1,1.742,1,4.246S16,25,16,25h26c0,0,1-1,1-4S42,17,42,17z"></path><path fill="#8BC34A" d="M18.609,41c0-0.293-0.113-0.584-0.36-0.781l-10.625-8c-0.43-0.345-1.061-0.274-1.405,0.156c-0.345,0.432-0.275,1.061,0.156,1.406L15.962,41H18.609z"></path><path fill="#8BC34A" d="M42,33H16c0,0,1,1.742,1,4.246S16,41,16,41h26c0,0,1-1,1-4S42,33,42,33z"></path><path fill="#689F38" d="M42,33H16c0,0,0.441,0.756,0.733,2h26.035C42.473,33.559,42,33,42,33z"></path><path fill="#303F9F" d="M42,25H16c0,0,0.441,0.756,0.733,2h26.035C42.473,25.559,42,25,42,25z"></path><path fill="#FDD835" d="M21.034 32c.11-.45.299-1.379.299-2.5s-.189-2.05-.299-2.5H19c.003.009.333 1.267.333 2.5S19.002 31.993 19 32H21.034zM21.034 24c.11-.45.299-1.379.299-2.5s-.189-2.05-.299-2.5H19c.003.009.333 1.267.333 2.5S19.002 23.993 19 24H21.034zM21.034 40c.11-.45.299-1.379.299-2.5s-.189-2.05-.299-2.5H19c.003.009.333 1.267.333 2.5S19.002 39.993 19 40H21.034z"></path><path fill="#7B1FA2" d="M42.768,19C42.473,17.559,42,17,42,17L30.844,8.591C30.844,8.591,30.063,8,29,8C27.771,8,7,8,7,8l0.018,0.018C6.703,8.012,6.39,8.137,6.191,8.412c-0.325,0.446-0.226,1.072,0.22,1.396l9.737,7.47c0.161,0.325,0.418,0.92,0.607,1.722H42.768z"></path><path fill="#AF7000" d="M32,16L21,8h-5l11,8c1.75,1.25,2,1.625,2,3s0,19,0,19s0,3-2,3c1.056,0,3.678,0,5,0c2,0,2-3,2-3s0-18,0-19C34,17,32.438,16.438,32,16z"></path><g><path fill="#FFC107" d="M34,27v4h-5v-4H34 M34.25,26h-5.5C28.336,26,28,26.336,28,26.75v4.5c0,0.414,0.336,0.75,0.75,0.75h5.5c0.414,0,0.75-0.336,0.75-0.75v-4.5C35,26.336,34.664,26,34.25,26L34.25,26z"></path></g><path fill="#5B3B07" d="M31.5 28.5A0.5 0.5 0 1 0 31.5 29.5A0.5 0.5 0 1 0 31.5 28.5Z"></path><path fill="#FFEB3B" d="M31.5,29c-0.276,0-0.5-0.224-0.5-0.5v-3c0-0.276,0.224-0.5,0.5-0.5s0.5,0.224,0.5,0.5v3C32,28.776,31.776,29,31.5,29z"></path></svg>`,

  // Arquivos compactados (zip, 7z, tar…) — ícone fornecido pelo usuário (Wikimedia).
  archive: `<svg xmlns="http://www.w3.org/2000/svg" fill="#FFF" viewBox="0 0 96 96"><path fill="#FFB900" d="m45 24-4.2426-4.2426C39.6321 18.6321 38.106 18 36.5147 18H9c-1.6569 0-3 1.3431-3 3v56c0 .5523.4477 1 1 1h82c.5523 0 1-.4477 1-1V27c0-1.6569-1.3431-3-3-3H45z"/><path fill="#FFD75E" d="m45 24-4.2426 4.2426C39.6321 29.3679 38.106 30 36.5147 30H6v47c0 .5523.4477 1 1 1h82c.5523 0 1-.4477 1-1V27c0-1.6569-1.3431-3-3-3H45z"/><linearGradient id="gdlZipGr" x1="48" x2="48" y1="24" y2="78" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#ffd75e" stop-opacity=".3"/></linearGradient><path fill="url(#gdlZipGr)" d="m45 24-4.2426 4.2426C39.6321 29.3679 38.106 30 36.5147 30H6v47c0 .5523.4477 1 1 1h82c.5523 0 1-.4477 1-1V27c0-1.6569-1.3431-3-3-3H45z"/><path d="M6 30v1h30.6005c1.8565 0 3.637-.7375 4.9497-2.0503L46.5 24H45l-4.2426 4.2426C39.6321 29.3679 38.106 30 36.5147 30H6z" opacity=".4"/><path fill="#DA7B16" d="M89 78H7c-.5523 0-1-.4477-1-1h84c0 .5523-.4477 1-1 1z"/><path fill="#E99E0C" d="M44 59c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zm5 0c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zm5 0c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zm5 0c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zm5 0c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zm5 0c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zm5 0c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zm5 0c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zm5 0c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zm5 0c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1s1 .4477 1 1v8c0 .5523-.4477 1-1 1zM38 47H11c-1.1046 0-2 .8954-2 2h2c.5523 0 1 .4477 1 1v8c0 .5523-.4477 1-1 1H9c0 1.1046.8954 2 2 2h27c1.1046 0 2-.8954 2-2V49c0-1.1046-.8955-2-2-2zm-1 11c0 .5523-.4477 1-1 1h-8c-.5523 0-1-.4477-1-1v-8c0-.5523.4477-1 1-1h8c.5523 0 1 .4477 1 1v8zM8 59H6V49h2c.5523 0 1 .4477 1 1v8c0 .5523-.4477 1-1 1z"/></svg>`,

  // Documento PDF.
  pdf: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#ffebee" d="M13 4h15l8 8v30a2 2 0 0 1-2 2H13a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z"/><path fill="#ef9a9a" d="M28 4l8 8h-8z"/><rect x="7" y="27" width="30" height="12" rx="2" fill="#e53935"/><text x="22" y="36" font-family="Arial, Helvetica, sans-serif" font-size="9" font-weight="700" fill="#fff" text-anchor="middle">PDF</text></svg>`,

  // MS Word — ícone oficial do icons8 (fornecido/licenciado pelo usuário).
  word: `<svg xmlns="http://www.w3.org/2000/svg" x="0px" y="0px" width="100" height="100" viewBox="0 0 48 48"><path fill="#2196F3" d="M41,10H25v28h16c0.553,0,1-0.447,1-1V11C42,10.447,41.553,10,41,10z"></path><path fill="#FFF" d="M25 15.001H39V17H25zM25 19H39V21H25zM25 23.001H39V25.001H25zM25 27.001H39V29H25zM25 31H39V33.001H25z"></path><path fill="#0D47A1" d="M27 42L6 38 6 10 27 6z"></path><path fill="#FFF" d="M21.167,31.012H18.45l-1.802-8.988c-0.098-0.477-0.155-0.996-0.174-1.576h-0.032c-0.043,0.637-0.11,1.162-0.197,1.576l-1.85,8.988h-2.827l-2.86-14.014h2.675l1.536,9.328c0.062,0.404,0.111,0.938,0.143,1.607h0.042c0.019-0.498,0.098-1.051,0.223-1.645l1.97-9.291h2.622l1.785,9.404c0.062,0.348,0.119,0.846,0.17,1.511h0.031c0.02-0.515,0.073-1.035,0.16-1.563l1.503-9.352h2.468L21.167,31.012z"></path></svg>`,

  // MS Excel — ícone oficial do icons8 (fornecido/licenciado pelo usuário).
  excel: `<svg xmlns="http://www.w3.org/2000/svg" x="0px" y="0px" width="100" height="100" viewBox="0 0 48 48"><path fill="#4CAF50" d="M41,10H25v28h16c0.553,0,1-0.447,1-1V11C42,10.447,41.553,10,41,10z"></path><path fill="#FFF" d="M32 15H39V18H32zM32 25H39V28H32zM32 30H39V33H32zM32 20H39V23H32zM25 15H30V18H25zM25 25H30V28H25zM25 30H30V33H25zM25 20H30V23H25z"></path><path fill="#2E7D32" d="M27 42L6 38 6 10 27 6z"></path><path fill="#FFF" d="M19.129,31l-2.411-4.561c-0.092-0.171-0.186-0.483-0.284-0.938h-0.037c-0.046,0.215-0.154,0.541-0.324,0.979L13.652,31H9.895l4.462-7.001L10.274,17h3.837l2.001,4.196c0.156,0.331,0.296,0.725,0.42,1.179h0.04c0.078-0.271,0.224-0.68,0.439-1.22L19.237,17h3.515l-4.199,6.939l4.316,7.059h-3.74V31z"></path></svg>`,

  // MS PowerPoint — ícone oficial do icons8 (fornecido/licenciado pelo usuário).
  powerpoint: `<svg xmlns="http://www.w3.org/2000/svg" x="0px" y="0px" width="100" height="100" viewBox="0 0 48 48"><path fill="#FF8A65" d="M41,10H25v28h16c0.553,0,1-0.447,1-1V11C42,10.447,41.553,10,41,10z"></path><path fill="#FBE9E7" d="M24 29H38V31H24zM24 33H38V35H24zM30 15c-3.313 0-6 2.687-6 6s2.687 6 6 6 6-2.687 6-6h-6V15z"></path><path fill="#FBE9E7" d="M32,13v6h6C38,15.687,35.313,13,32,13z"></path><path fill="#E64A19" d="M27 42L6 38 6 10 27 6z"></path><path fill="#FFF" d="M16.828,17H12v14h3v-4.823h1.552c1.655,0,2.976-0.436,3.965-1.304c0.988-0.869,1.484-2.007,1.482-3.412C22,18.487,20.275,17,16.828,17z M16.294,23.785H15v-4.364h1.294c1.641,0,2.461,0.72,2.461,2.158C18.755,23.051,17.935,23.785,16.294,23.785z"></path></svg>`,

  // Áudio (mp3, flac…) — ícone do icons8 (fornecido/licenciado pelo usuário).
  audio: `<svg xmlns="http://www.w3.org/2000/svg" x="0px" y="0px" width="100" height="100" viewBox="0 0 48 48"><path fill="#ed3675" d="M20,24c-5.523,0-10,4.477-10,10s4.477,10,10,10s10-4.477,10-10S25.523,24,20,24z"></path><linearGradient id="gdlAudioGr1" x1="30" x2="41" y1="8" y2="8" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#bd1949"></stop><stop offset=".108" stop-color="#c31a4b"></stop><stop offset=".38" stop-color="#ca1b4d"></stop><stop offset="1" stop-color="#cc1b4e"></stop></linearGradient><path fill="url(#gdlAudioGr1)" d="M39,12h-9V4h9c1.105,0,2,0.895,2,2v4C41,11.105,40.105,12,39,12z"></path><path fill="#ed3675" d="M30,4h-2c-2.209,0-4,1.791-4,4v26h6V4z"></path></svg>`,

  image: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="6" y="9" width="36" height="30" rx="4" fill="#009688"/><rect x="9" y="12" width="30" height="24" rx="2" fill="#b2dfdb"/><circle cx="18" cy="20" r="3.5" fill="#fff59d"/><path fill="#00897b" d="M12 36l8-9 5 5.5 6-7.5 6 11z"/></svg>`,

  disk: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><circle cx="24" cy="24" r="19" fill="#607d8b"/><path fill="#90a4ae" d="M24 5a19 19 0 0 1 15.6 8.2l-6 4.2A11.7 11.7 0 0 0 24 12.3z"/><circle cx="24" cy="24" r="6.5" fill="#eceff1"/><circle cx="24" cy="24" r="2.3" fill="#607d8b"/></svg>`,

  windows: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#039be5" d="M7 9l15-2.1V22H7z"/><path fill="#4fc3f7" d="M24 6.6L41 4v18H24z"/><path fill="#0288d1" d="M7 26h15v13.1L7 37z"/><path fill="#29b6f6" d="M24 26h17v18l-17-2.4z"/></svg>`,

  android: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#aed581" d="M14 18a10 10 0 0 1 20 0z"/><path fill="#33691e" d="M18.5 13.2l-2.2-3.4a.7.7 0 0 1 1.2-.75l2.25 3.5a12 12 0 0 1 8.5 0l2.25-3.5a.7.7 0 1 1 1.2.75l-2.2 3.4"/><circle cx="19" cy="15" r="1.4" fill="#fff"/><circle cx="29" cy="15" r="1.4" fill="#fff"/><path fill="#7cb342" d="M13 20h22v13a3 3 0 0 1-3 3H16a3 3 0 0 1-3-3z"/><rect x="6.5" y="20" width="4.5" height="13" rx="2.25" fill="#7cb342"/><rect x="37" y="20" width="4.5" height="13" rx="2.25" fill="#7cb342"/><rect x="16.5" y="35" width="4.5" height="9" rx="2.25" fill="#7cb342"/><rect x="27" y="35" width="4.5" height="9" rx="2.25" fill="#7cb342"/></svg>`,

  code: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="6" y="8" width="36" height="32" rx="5" fill="#37474f"/><rect x="6" y="8" width="36" height="7" rx="5" fill="#455a64"/><circle cx="11" cy="11.5" r="1.2" fill="#ff5f56"/><circle cx="15" cy="11.5" r="1.2" fill="#ffbd2e"/><circle cx="19" cy="11.5" r="1.2" fill="#27c93f"/><path fill="none" stroke="#4dd0e1" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" d="M18 22l-5 5 5 5M30 22l5 5-5 5M27 20l-4 14"/></svg>`,

  text: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#fafafa" d="M13 4h15l8 8v30a2 2 0 0 1-2 2H13a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z"/><path fill="#cfd8dc" d="M28 4l8 8h-8z"/><path stroke="#90a4ae" stroke-width="2" stroke-linecap="round" d="M16 21h13M16 26h13M16 31h8"/></svg>`,

  subtitle: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="5" y="11" width="38" height="26" rx="5" fill="#455a64"/><rect x="10" y="26" width="13" height="4" rx="2" fill="#fff176"/><rect x="26" y="26" width="12" height="4" rx="2" fill="#fff176"/><rect x="10" y="19" width="8" height="3.5" rx="1.75" fill="#b0bec5"/><rect x="21" y="19" width="17" height="3.5" rx="1.75" fill="#b0bec5"/></svg>`,

  // E-book (epub, mobi, azw…) — livro aberto.
  ebook: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><path fill="#8d6e63" d="M6 11c5-2.5 11-2.5 18 0v27c-7-2.5-13-2.5-18 0z"/><path fill="#6d4c41" d="M42 11c-5-2.5-11-2.5-18 0v27c7-2.5 13-2.5 18 0z"/><path fill="#efebe9" d="M9 14c4-1.7 8.5-1.7 13 0v22c-4.5-1.7-9-1.7-13 0zM39 14c-4-1.7-8.5-1.7-13 0v22c4.5-1.7 9-1.7 13 0z"/><path stroke="#bcaaa4" stroke-width="1.5" stroke-linecap="round" d="M12 19h7M12 23h7M29 19h7M29 23h7"/></svg>`,

  // Fonte tipográfica (ttf, otf, woff…).
  font: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect x="6" y="6" width="36" height="36" rx="6" fill="#3f51b5"/><path fill="#fff" d="M15 15h18v4h-6.8v14h-4.4V19H15z"/><path fill="#c5cae9" d="M28 27h9v3.2h-3.4V38h-2.4v-7.8H28z"/></svg>`,

  // Torrent (.torrent).
  torrent: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><circle cx="24" cy="24" r="19" fill="#5c6bc0"/><path fill="none" stroke="#fff" stroke-width="3.4" stroke-linecap="round" d="M24 10v13a9 9 0 0 0 9 9h4"/><path fill="none" stroke="#c5cae9" stroke-width="3.4" stroke-linecap="round" d="M24 19a5 5 0 0 0 5 5h4"/></svg>`,

  // Banco de dados (sql, db, sqlite…).
  database: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><ellipse cx="24" cy="12" rx="15" ry="6" fill="#ff8f00"/><path fill="#ffa000" d="M9 12v10c0 3.3 6.7 6 15 6s15-2.7 15-6V12c0 3.3-6.7 6-15 6S9 15.3 9 12z"/><path fill="#ffb300" d="M9 22v10c0 3.3 6.7 6 15 6s15-2.7 15-6V22c0 3.3-6.7 6-15 6S9 25.3 9 22z"/></svg>`,
}

// Extensão → chave de app. Cobre os formatos mais comuns de download.
const EXT_TO_APP: Record<string, string> = {
  // Vídeo → VLC
  mkv: 'vlc', mp4: 'vlc', avi: 'vlc', mov: 'vlc', wmv: 'vlc', m4v: 'vlc', flv: 'vlc',
  webm: 'vlc', mpg: 'vlc', mpeg: 'vlc', vob: 'vlc', ogv: 'vlc', '3gp': 'vlc', ts: 'vlc',
  mts: 'vlc', m2ts: 'vlc', rmvb: 'vlc', rm: 'vlc', asf: 'vlc', divx: 'vlc',
  // Arquivos
  rar: 'winrar', r00: 'winrar', r01: 'winrar',
  zip: 'archive', '7z': 'archive', tar: 'archive', gz: 'archive', tgz: 'archive',
  bz2: 'archive', xz: 'archive', zst: 'archive', lz: 'archive', cab: 'archive', arj: 'archive',
  // Documentos office
  pdf: 'pdf',
  doc: 'word', docx: 'word', odt: 'word', rtf: 'word',
  xls: 'excel', xlsx: 'excel', ods: 'excel', csv: 'excel',
  ppt: 'powerpoint', pptx: 'powerpoint', odp: 'powerpoint',
  // Mídia
  mp3: 'audio', flac: 'audio', aac: 'audio', ogg: 'audio', wav: 'audio', opus: 'audio',
  m4a: 'audio', wma: 'audio', aiff: 'audio', alac: 'audio', mid: 'audio', midi: 'audio',
  jpg: 'image', jpeg: 'image', png: 'image', gif: 'image', webp: 'image', bmp: 'image',
  tif: 'image', tiff: 'image', heic: 'image', heif: 'image', avif: 'image', svg: 'image',
  psd: 'image', ai: 'image', ico: 'image', raw: 'image',
  // Discos / instaladores
  iso: 'disk', img: 'disk', dmg: 'disk', vhd: 'disk', vhdx: 'disk', vmdk: 'disk',
  exe: 'windows', msi: 'windows', bat: 'windows', cmd: 'windows',
  apk: 'android', xapk: 'android', aab: 'android',
  // E-books / fontes / torrent / banco de dados
  epub: 'ebook', mobi: 'ebook', azw: 'ebook', azw3: 'ebook', fb2: 'ebook', cbz: 'ebook', cbr: 'ebook',
  ttf: 'font', otf: 'font', woff: 'font', woff2: 'font', eot: 'font',
  torrent: 'torrent',
  sql: 'database', db: 'database', sqlite: 'database', sqlite3: 'database', db3: 'database', mdb: 'database',
  // Legendas / texto / código
  srt: 'subtitle', vtt: 'subtitle', ass: 'subtitle', ssa: 'subtitle', sub: 'subtitle',
  txt: 'text', md: 'text', log: 'text', nfo: 'text', ini: 'text', cfg: 'text',
  json: 'code', xml: 'code', js: 'code', py: 'code', rs: 'code', html: 'code',
  css: 'code', sh: 'code', c: 'code', cpp: 'code', java: 'code', go: 'code', php: 'code',
}

function extensionOf(filename: string): string {
  const clean = filename.split('?')[0].split('#')[0].trim()
  const dot = clean.lastIndexOf('.')
  if (dot < 0 || dot === clean.length - 1) return ''
  return clean.slice(dot + 1).toLowerCase()
}

// Retorna o ícone de app para o formato do arquivo, ou null se não houver mapeamento.
export function getFileTypeAppIcon(filename: string): FileTypeIcon | null {
  const app = EXT_TO_APP[extensionOf(filename)]
  if (!app) return null
  const svg = APP_ICONS[app]
  if (!svg) return null
  return { app, svg }
}
