package models

// Portado de backend/src/models.rs:13 — mantém snake_case no JSON para compatibilidade com Vue/Rust
type DownloadStatus string

const (
	StatusPending        DownloadStatus = "pending"
	StatusDownloading    DownloadStatus = "downloading"
	StatusVerifying      DownloadStatus = "verifying"
	StatusPaused         DownloadStatus = "paused"
	StatusComplete       DownloadStatus = "complete"
	StatusCorrupted      DownloadStatus = "corrupted"
	StatusError          DownloadStatus = "error"
	StatusCancelled      DownloadStatus = "cancelled"
	StatusRateLimited    DownloadStatus = "rate_limited"
	StatusWaitingCaptcha DownloadStatus = "waiting_captcha"
	StatusDiskFull       DownloadStatus = "disk_full"
)

type HashAlgorithm string

const (
	HashAlgoMd5    HashAlgorithm = "md5"
	HashAlgoSha1   HashAlgorithm = "sha1"
	HashAlgoSha256 HashAlgorithm = "sha256"
	HashAlgoCrc32  HashAlgorithm = "crc32"
)

type ExpectedHash struct {
	Algorithm HashAlgorithm `json:"algorithm"`
	Value     string        `json:"value"`
}

type Download struct {
	ID                  string                `json:"id"`
	URL                 string                `json:"url"`
	Provider            string                `json:"provider"`
	IdentityKey         string                `json:"identityKey"`
	Filename            string                `json:"filename"`
	Size                uint64                `json:"size"`
	DestPath            string                `json:"destPath"`
	Status              DownloadStatus        `json:"status"`
	BytesDownloaded     uint64                `json:"bytesDownloaded"`
	SpeedBps            uint64                `json:"speedBps"`
	EtaSecs             uint64                `json:"etaSecs"`
	DurationSecs        *uint64               `json:"durationSecs,omitempty"`
	IsFolder            bool                  `json:"isFolder"`
	Children            *[]FileChildInfo      `json:"children,omitempty"`
	RetryCount          uint32                `json:"retryCount"`
	MaxRetries          uint32                `json:"maxRetries"`
	SpeedLimitKib       uint64                `json:"speedLimitKib"`
	ParallelParts       uint32                `json:"parallelParts"`
	SelectedChildren    *[]string             `json:"selectedChildren,omitempty"`
	ExpectedHashValue   *ExpectedHash         `json:"expectedHash,omitempty"`
	RetryAt             *uint64               `json:"retryAt,omitempty"`
	CaptchaType         *string               `json:"captchaType,omitempty"`
	CaptchaSitekey      *string               `json:"captchaSitekey,omitempty"`
	CaptchaPageURL      *string               `json:"captchaPageUrl,omitempty"`
	CaptchaToken        *string               `json:"captchaToken,omitempty"`
	Error               *string               `json:"error,omitempty"`
	ErrorKind           *string               `json:"errorKind,omitempty"`
	Priority            int32                 `json:"priority"`
	CreatedAt           uint64                `json:"createdAt"`
	StartedAt           *uint64               `json:"startedAt,omitempty"`
	CompletedAt         *uint64               `json:"completedAt,omitempty"`
	LastProgressAt      *uint64               `json:"lastProgressAt,omitempty"`
	Pinned              bool                  `json:"pinned"`
	PackageID           *string               `json:"packageId,omitempty"`
	RequestHeaders      *map[string]string    `json:"requestHeaders,omitempty"`
	NetworkRoute        *DownloadNetworkRoute `json:"networkRoute,omitempty"`
	ThumbnailURL        *string               `json:"thumbnailUrl,omitempty"`
	ThumbnailData       *string               `json:"thumbnailData,omitempty"`
	ChannelName         *string               `json:"channelName,omitempty"`
	ChannelThumbnailURL *string               `json:"channelThumbnailUrl,omitempty"`
	AutoTorOnLimit      bool                  `json:"autoTorOnLimit"`
}

type DownloadNetworkRoute struct {
	Mode            string  `json:"mode"`
	Isolated        bool    `json:"isolated"`
	ProxyHost       string  `json:"proxyHost"`
	ProxyPort       uint16  `json:"proxyPort"`
	ProxyUsername   *string `json:"proxyUsername,omitempty"`
	ProxyPassword   *string `json:"proxyPassword,omitempty"`
	ExitIP          *string `json:"exitIp,omitempty"`
	ExitCountry     *string `json:"exitCountry,omitempty"`
	ExitCountryCode *string `json:"exitCountryCode,omitempty"`
	CircuitChanges  uint32  `json:"circuitChanges"`
	LastCheckedAt   *uint64 `json:"lastCheckedAt,omitempty"`
}

type FileChildInfo struct {
	Filename        string          `json:"filename"`
	Size            uint64          `json:"size"`
	MimeType        *string         `json:"mimeType,omitempty"`
	IsFolder        bool            `json:"isFolder"`
	Path            *string         `json:"path,omitempty"`
	SourceURL       *string         `json:"sourceUrl,omitempty"`
	BytesDownloaded *uint64         `json:"bytesDownloaded,omitempty"`
	SpeedBps        *uint64         `json:"speedBps,omitempty"`
	EtaSecs         *uint64         `json:"etaSecs,omitempty"`
	Status          *DownloadStatus `json:"status,omitempty"`
}

type FileInfo struct {
	Filename            string           `json:"filename"`
	Size                uint64           `json:"size"`
	DurationSecs        *uint64          `json:"durationSecs,omitempty"`
	MimeType            *string          `json:"mimeType,omitempty"`
	IsFolder            bool             `json:"isFolder"`
	Children            *[]FileChildInfo `json:"children,omitempty"`
	ThumbnailURL        *string          `json:"thumbnailUrl,omitempty"`
	ChannelName         *string          `json:"channelName,omitempty"`
	ChannelThumbnailURL *string          `json:"channelThumbnailUrl,omitempty"`
}

type SecureSettings struct {
	NopechaAPIKey  *string               `json:"nopechaApiKey,omitempty"`
	TeraboxAccount *TeraboxAccountSecret `json:"teraboxAccount,omitempty"`
}

type TeraboxAccountSecret struct {
	Email      string   `json:"email"`
	Password   string   `json:"password"`
	Cookies    []string `json:"cookies"`
	VerifiedAt *string  `json:"verifiedAt,omitempty"`
}

type RemoteAccessSettings struct {
	Enabled  bool   `json:"enabled"`
	AllowLan bool   `json:"allowLan"`
	Username string `json:"username"`
	Password string `json:"password"`
	Port     uint16 `json:"port"`
}

type DownloadListFilters struct {
	Statuses []string `json:"statuses"`
	Hosts    []string `json:"hosts"`
	Packages []string `json:"packages"`
}

type PublicSettings struct {
	Theme                     string               `json:"theme"`
	Locale                    string               `json:"locale"`
	OutputDir                 string               `json:"outputDir"`
	MaxConcurrentDownloads    int                  `json:"maxConcurrentDownloads"`
	MaxRetriesPerDownload     uint32               `json:"maxRetriesPerDownload"`
	SpeedLimitKib             uint64               `json:"speedLimitKib"`
	ParallelPartsPerDownload  uint32               `json:"parallelPartsPerDownload"`
	FontSize                  uint32               `json:"fontSize"`
	FontFamily                string               `json:"fontFamily"`
	UiZoom                    float32              `json:"uiZoom"`
	NativeNotification        bool                 `json:"nativeNotification"`
	ClipboardMonitorEnabled   bool                 `json:"clipboardMonitorEnabled"`
	AccentColor               *string              `json:"accentColor,omitempty"`
	ProxyMode                 string               `json:"proxyMode"`
	ProxyHost                 string               `json:"proxyHost"`
	ProxyPort                 uint16               `json:"proxyPort"`
	ProxyUsername             *string              `json:"proxyUsername,omitempty"`
	ProxyPassword             *string              `json:"proxyPassword,omitempty"`
	StartTor                  bool                 `json:"startTor"`
	ReservedDiskMb            uint64               `json:"reservedDiskMb"`
	UseReconnectOnRateLimit   bool                 `json:"useReconnectOnRateLimit"`
	ReconnectMethod           string               `json:"reconnectMethod"`
	ReconnectCommand          string               `json:"reconnectCommand"`
	RouterIP                  string               `json:"routerIp"`
	PostDownloadAction        string               `json:"postDownloadAction"`
	PostDownloadActionTrigger string               `json:"postDownloadActionTrigger"`
	PostDownloadCommand       string               `json:"postDownloadCommand"`
	PostDownloadWebhookURL    string               `json:"postDownloadWebhookUrl"`
	AutoExtract               bool                 `json:"autoExtract"`
	PasswordList              []string             `json:"passwordList"`
	DuplicateAction           string               `json:"duplicateAction"`
	RemoteAccess              RemoteAccessSettings `json:"remoteAccess"`
	VisibleColumns            []string             `json:"visibleColumns"`
	LastFilters               DownloadListFilters  `json:"lastFilters"`
	UiDensity                 string               `json:"uiDensity"`
	InterceptMode             string               `json:"interceptMode"`
	InterceptMinSizeMb        uint64               `json:"interceptMinSizeMb"`
	InterceptMimeAllowlist    []string             `json:"interceptMimeAllowlist"`
	InterceptDomainBlocklist  []string             `json:"interceptDomainBlocklist"`
	InterceptAskBeforeAdd     bool                 `json:"interceptAskBeforeAdd"`
	OnboardingCompleted       bool                 `json:"onboardingCompleted"`
	YoutubeUseCookies         bool                 `json:"youtubeUseCookies"`
	YoutubeCookieBrowser      string               `json:"youtubeCookieBrowser"`
	YoutubeCookiesFile        string               `json:"youtubeCookiesFile"`
	YoutubeMergeFormat        string               `json:"youtubeMergeFormat"`
	YoutubeDownloadSubs       bool                 `json:"youtubeDownloadSubs"`
	YoutubeSubLangs           string               `json:"youtubeSubLangs"`
	YoutubeEmbedSubs          bool                 `json:"youtubeEmbedSubs"`
	YoutubeSplitChapters      bool                 `json:"youtubeSplitChapters"`
	YoutubeDownloadPack       bool                 `json:"youtubeDownloadPack"`
	InfiniteRetries           bool                 `json:"infiniteRetries"`
}

type HistoryItem struct {
	ID         string  `json:"id"`
	URL        string  `json:"url"`
	Title      string  `json:"title"`
	Host       string  `json:"host"`
	Thumbnail  string  `json:"thumbnail"`
	Date       string  `json:"date"`
	FormatID   string  `json:"formatId"`
	OutputPath *string `json:"outputPath,omitempty"`
	Sha256Hash *string `json:"sha256Hash,omitempty"`
}

type Package struct {
	ID              string  `json:"id"`
	Name            string  `json:"name"`
	Color           string  `json:"color"`
	Comment         *string `json:"comment,omitempty"`
	DestDirOverride *string `json:"destDirOverride,omitempty"`
	Priority        int32   `json:"priority"`
	CreatedAt       uint64  `json:"createdAt"`
}

type ApiError struct {
	Error     string  `json:"error"`
	Duplicate *string `json:"duplicate,omitempty"`
}

type LegacyConfigMigration struct {
	Version   int64  `json:"version"`
	Name      string `json:"name"`
	AppliedAt uint64 `json:"appliedAt"`
}

type CachedFileInfo struct {
	URL                 string           `json:"url"`
	ProviderID          string           `json:"providerId"`
	Name                string           `json:"name"`
	Size                uint64           `json:"size"`
	DurationSecs        *uint64          `json:"durationSecs,omitempty"`
	MimeType            *string          `json:"mimeType,omitempty"`
	IsFolder            bool             `json:"isFolder"`
	Children            *[]FileChildInfo `json:"children,omitempty"`
	ThumbnailURL        *string          `json:"thumbnailUrl,omitempty"`
	ChannelName         *string          `json:"channelName,omitempty"`
	ChannelThumbnailURL *string          `json:"channelThumbnailUrl,omitempty"`
	CachedAt            uint64           `json:"cachedAt"`
	LastCheckedAt       *uint64          `json:"lastCheckedAt,omitempty"`
}

type DownloadEvent struct {
	ID         int64  `json:"id"`
	DownloadID string `json:"downloadId"`
	Kind       string `json:"kind"`
	Message    string `json:"message"`
	CreatedAt  uint64 `json:"createdAt"`
}

type ArchivePassword struct {
	Password     string  `json:"password"`
	SuccessCount uint64  `json:"successCount"`
	LastUsedAt   *uint64 `json:"lastUsedAt,omitempty"`
	Source       string  `json:"source"`
}

type InterceptHistoryItem struct {
	ID       string `json:"id"`
	URL      string `json:"url"`
	Filename string `json:"filename"`
	MimeType string `json:"mimeType"`
	Size     uint64 `json:"size"`
	Status   string `json:"status"`
	CreatedAt uint64 `json:"createdAt"`
}
