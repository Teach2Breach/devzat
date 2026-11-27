package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"image"
	_ "image/gif"
	_ "image/jpeg"
	"image/png"
	"io"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"syscall"
	"time"

	"github.com/disintegration/imaging"
	_ "golang.org/x/image/webp"
)

const maxImageBytes = 8 << 20

var (
	dataURIRe  = regexp.MustCompile(`data:image/(?:png|jpe?g|gif|webp);base64,[A-Za-z0-9+/=\r\n]+`)
	mdDataRe   = regexp.MustCompile(`!\[([^\]]*)\]\(\s*(data:image/(?:png|jpe?g|gif|webp);base64,[A-Za-z0-9+/=\r\n]+)\s*\)`)
	itermRe    = regexp.MustCompile(`\x1b\]1337;File=[^:\x07\x1b]*:([A-Za-z0-9+/=\r\n]+)(?:\x07|\x1b\\)?`)
	hqBangRe   = regexp.MustCompile(`!hq\[([^\]]*)\]\(([^)\s]+)\)`)
	hqAltRe    = regexp.MustCompile(`!\[hq(?::\s*([^\]]*))?\]\(([^)\s]+)\)`)
	uploadRe   = regexp.MustCompile(`^[a-f0-9]{64}\.(png|jpg|gif|webp|img)$`)
	imageURLRe = regexp.MustCompile(`https?://[^\s<>'"()]+?\.(?:gif|png|jpe?g|webp)(?:\?[^\s<>'"()]*)?`)
	hqEmptyPre = regexp.MustCompile(`(?i)(?:^|\s)!hq\[\]\s*$`)
	hqWordPre  = regexp.MustCompile(`(?i)(?:^|\s)!hq\s*$`)
	altWordPre = regexp.MustCompile(`(?:^|\s)!([A-Za-z][A-Za-z0-9_-]{0,30})\s*$`)
)

type rawImage struct {
	data []byte
	ext  string
}

// markHQImages rewrites !hq[](url) and ![hq](url) into a normal image whose
// URL is prefixed so the renderer can tell it should be high resolution.
func markHQImages(md string) string {
	md = hqBangRe.ReplaceAllStringFunc(md, func(m string) string {
		parts := hqBangRe.FindStringSubmatch(m)
		if len(parts) != 3 {
			return m
		}
		return "![" + strings.TrimSpace(parts[1]) + "](" + prefixHQ(parts[2]) + ")"
	})
	md = hqAltRe.ReplaceAllStringFunc(md, func(m string) string {
		parts := hqAltRe.FindStringSubmatch(m)
		if len(parts) != 3 {
			return m
		}
		return "![" + strings.TrimSpace(parts[1]) + "](" + prefixHQ(parts[2]) + ")"
	})
	return md
}

// normalizeImageLinks turns a bare picture URL into a markdown image.
// "!hq[]https://x.jpg", "!hq https://x.jpg", and "!cat https://x.jpg" all count.
// A link ending in png, jpg, jpeg, gif, or webp is enough; the parentheses are optional.
func normalizeImageLinks(text string) string {
	locs := imageURLRe.FindAllStringIndex(text, -1)
	if len(locs) == 0 {
		return text
	}
	var b strings.Builder
	last := 0
	for _, loc := range locs {
		start, end := loc[0], loc[1]
		if start > 0 && text[start-1] == '(' {
			b.WriteString(text[last:end])
			last = end
			continue
		}
		url := text[start:end]
		if strings.Contains(url, "://example.com/") || strings.Contains(url, "://example.org/") {
			b.WriteString(text[last:end])
			last = end
			continue
		}
		head := text[last:start]
		hq := false
		alt := ""
		switch {
		case hqEmptyPre.MatchString(head):
			head = cutImagePrefix(head, hqEmptyPre)
			hq = true
		case hqWordPre.MatchString(head):
			head = cutImagePrefix(head, hqWordPre)
			hq = true
		case altWordPre.MatchString(head):
			alt = altWordPre.FindStringSubmatch(head)[1]
			head = cutImagePrefix(head, altWordPre)
		}
		b.WriteString(head)
		if hq {
			b.WriteString("!hq[](" + url + ")")
		} else {
			b.WriteString("![" + alt + "](" + url + ")")
		}
		last = end
	}
	b.WriteString(text[last:])
	return b.String()
}

func cutImagePrefix(head string, re *regexp.Regexp) string {
	loc := re.FindStringIndex(head)
	if loc == nil {
		return head
	}
	kept := head[:loc[0]]
	if kept != "" && !strings.HasSuffix(kept, " ") && !strings.HasSuffix(kept, "\n") {
		kept += " "
	}
	return kept
}

func prefixHQ(url string) string {
	if strings.HasPrefix(url, "dzhq:") {
		return url
	}
	return "dzhq:" + url
}

func splitHQRef(ref string) (string, bool) {
	if strings.HasPrefix(ref, "dzhq:") {
		return strings.TrimPrefix(ref, "dzhq:"), true
	}
	return ref, false
}

// absorbEmbeddedImages stores data-URI pictures on disk and replaces them with
// short devzat-file links so history and other clients don't carry the bytes.
func absorbEmbeddedImages(text string) string {
	text = normalizeImageLinks(text)
	if !strings.Contains(text, "data:image/") {
		return text
	}
	trimmed := strings.TrimSpace(text)
	if dataURIRe.MatchString(trimmed) && dataURIRe.FindString(trimmed) == trimmed {
		if name, ok := storeDataURI(trimmed); ok {
			return "![](devzat-file:" + name + ")"
		}
	}
	return dataURIRe.ReplaceAllStringFunc(text, func(m string) string {
		name, ok := storeDataURI(m)
		if !ok {
			return m
		}
		return "devzat-file:" + name
	})
}

func storeDataURI(uri string) (string, bool) {
	raw, ext, ok := decodeDataURI(uri)
	if !ok {
		return "", false
	}
	name, err := saveUpload(raw, ext)
	if err != nil {
		Log.Println("image:", err)
		return "", false
	}
	return name, true
}

func saveUpload(data []byte, ext string) (string, error) {
	if ext == "" || ext[0] != '.' {
		ext = ".img"
	}
	sum := sha256.Sum256(data)
	name := hex.EncodeToString(sum[:]) + ext
	if !uploadRe.MatchString(name) {
		return "", fmt.Errorf("refusing image name %q", name)
	}
	dir := filepath.Join(Config.DataDir, "uploads")
	if err := os.MkdirAll(dir, 0755); err != nil {
		return "", err
	}
	path := filepath.Join(dir, name)
	if _, err := os.Stat(path); err != nil {
		if err := os.WriteFile(path, data, 0644); err != nil {
			return "", err
		}
	} else {
		now := time.Now() // a repost keeps the file alive as long as the newest message that uses it
		_ = os.Chtimes(path, now, now)
	}
	return name, nil
}

// pruneUploads deletes uploaded images older than the history window, so they live exactly as long as the chat that shows them.
func pruneUploads() {
	if Config.HistoryHours <= 0 {
		return
	}
	dir := filepath.Join(Config.DataDir, "uploads")
	entries, err := os.ReadDir(dir)
	if err != nil {
		return
	}
	cut := time.Now().Add(-time.Duration(Config.HistoryHours)*time.Hour - time.Hour)
	for _, e := range entries {
		if !uploadRe.MatchString(e.Name()) {
			continue
		}
		info, err := e.Info()
		if err == nil && info.ModTime().Before(cut) {
			_ = os.Remove(filepath.Join(dir, e.Name()))
		}
	}
}

func loadUpload(name string) ([]byte, error) {
	if !uploadRe.MatchString(name) || strings.Contains(name, "/") {
		return nil, fmt.Errorf("invalid image")
	}
	return os.ReadFile(filepath.Join(Config.DataDir, "uploads", name))
}

func decodeDataURI(uri string) ([]byte, string, bool) {
	i := strings.Index(uri, "data:image/")
	if i < 0 {
		return nil, "", false
	}
	uri = uri[i:]
	comma := strings.Index(uri, ",")
	if comma < 0 {
		return nil, "", false
	}
	meta := uri[:comma]
	if !strings.Contains(meta, ";base64") {
		return nil, "", false
	}
	raw, err := decodeB64(uri[comma+1:])
	if err != nil || len(raw) == 0 || len(raw) > maxImageBytes {
		return nil, "", false
	}
	if _, format, ok := decodeImageBytes(raw); !ok {
		return nil, "", false
	} else {
		return raw, extForFormat(format), true
	}
}

func decodeB64(s string) ([]byte, error) {
	s = strings.Map(func(r rune) rune {
		switch r {
		case '\n', '\r', '\t', ' ':
			return -1
		default:
			return r
		}
	}, s)
	if s == "" {
		return nil, fmt.Errorf("empty")
	}
	if m := len(s) % 4; m != 0 {
		s += strings.Repeat("=", 4-m)
	}
	return base64.StdEncoding.DecodeString(s)
}

func decodeImageBytes(b []byte) (image.Image, string, bool) {
	if len(b) == 0 || len(b) > maxImageBytes {
		return nil, "", false
	}
	cfg, format, err := image.DecodeConfig(bytes.NewReader(b))
	if err != nil || cfg.Width <= 0 || cfg.Height <= 0 || cfg.Width > 4032*2 || cfg.Height > 3024*2 {
		return nil, "", false
	}
	img, format2, err := image.Decode(bytes.NewReader(b))
	if err != nil {
		return nil, "", false
	}
	if format == "" {
		format = format2
	}
	return img, format, true
}

func extForFormat(format string) string {
	switch format {
	case "png":
		return ".png"
	case "jpeg":
		return ".jpg"
	case "gif":
		return ".gif"
	case "webp":
		return ".webp"
	default:
		return ".img"
	}
}

// extractPastedImages pulls pictures out of a bracketed paste or an iTerm
// inline-image sequence. caption is whatever text was around them.
func extractPastedImages(body []byte) (caption string, imgs []rawImage, hq bool, ok bool) {
	s := string(body)
	hq = strings.Contains(s, "!hq[") || strings.Contains(s, "![hq]") || strings.Contains(s, "![hq:")

	type span struct {
		start, end int
		img        rawImage
		alt        string
	}
	var spans []span
	add := func(start, end int, raw []byte, format, alt string) {
		if start < 0 || end > len(s) || start >= end {
			return
		}
		for _, existing := range spans {
			if start < existing.end && end > existing.start {
				return
			}
		}
		spans = append(spans, span{start, end, rawImage{data: raw, ext: extForFormat(format)}, alt})
	}

	for _, m := range mdDataRe.FindAllStringSubmatchIndex(s, -1) {
		// groups: 0 full, 1 alt, 2 uri
		raw, format, good := decodeURIBytes(s[m[4]:m[5]])
		if !good {
			continue
		}
		alt := s[m[2]:m[3]]
		if alt == "hq" || strings.HasPrefix(alt, "hq:") {
			hq = true
			alt = strings.TrimSpace(strings.TrimPrefix(alt, "hq:"))
		}
		add(m[0], m[1], raw, format, alt)
	}
	for _, m := range dataURIRe.FindAllStringIndex(s, -1) {
		raw, format, good := decodeURIBytes(s[m[0]:m[1]])
		if !good {
			continue
		}
		add(m[0], m[1], raw, format, "")
	}
	for _, m := range itermRe.FindAllStringSubmatchIndex(s, -1) {
		raw, err := decodeB64(s[m[2]:m[3]])
		if err != nil {
			continue
		}
		_, format, good := decodeImageBytes(raw)
		if !good {
			continue
		}
		add(m[0], m[1], raw, format, "")
	}

	if len(spans) == 0 {
		trim := bytes.TrimSpace(body)
		if _, format, good := decodeWholeImage(trim); good {
			return "", []rawImage{{data: append([]byte(nil), trim...), ext: extForFormat(format)}}, hq, true
		}
		compact := strings.Map(func(r rune) rune {
			switch r {
			case '\n', '\r', '\t', ' ':
				return -1
			default:
				return r
			}
		}, string(trim))
		if len(compact) >= 80 && isB64Alphabet(compact) {
			raw, err := decodeB64(compact)
			if err == nil {
				if _, format, good := decodeWholeImage(raw); good {
					return "", []rawImage{{data: raw, ext: extForFormat(format)}}, hq, true
				}
			}
		}
		return "", nil, false, false
	}

	// Spans were appended in find order, not position order.
	for i := 1; i < len(spans); i++ {
		j := i
		for j > 0 && spans[j].start < spans[j-1].start {
			spans[j], spans[j-1] = spans[j-1], spans[j]
			j--
		}
	}

	var b strings.Builder
	last := 0
	alts := make([]string, 0, 1)
	for _, sp := range spans {
		if sp.start < last {
			continue
		}
		b.WriteString(s[last:sp.start])
		last = sp.end
		imgs = append(imgs, sp.img)
		if sp.alt != "" && sp.alt != "image" {
			alts = append(alts, sp.alt)
		}
	}
	b.WriteString(s[last:])
	caption = cleanCaption(b.String())
	if caption == "" && len(alts) > 0 {
		caption = strings.Join(alts, " ")
	}
	return caption, imgs, hq, true
}

func decodeWholeImage(b []byte) (image.Image, string, bool) {
	img, format, ok := decodeImageBytes(b)
	if !ok {
		return nil, "", false
	}
	return img, format, true
}

func decodeURIBytes(uri string) ([]byte, string, bool) {
	raw, ext, ok := decodeDataURI(uri)
	if !ok {
		return nil, "", false
	}
	format := strings.TrimPrefix(ext, ".")
	if format == "jpg" {
		format = "jpeg"
	}
	return raw, format, true
}

func isB64Alphabet(s string) bool {
	for _, r := range s {
		switch {
		case r >= 'A' && r <= 'Z', r >= 'a' && r <= 'z', r >= '0' && r <= '9', r == '+', r == '/', r == '=':
		default:
			return false
		}
	}
	return true
}

func cleanCaption(s string) string {
	s = strings.ReplaceAll(s, "\r", " ")
	s = strings.ReplaceAll(s, "\n", " ")
	s = strings.ReplaceAll(s, "!hq", " ")
	s = strings.TrimSpace(s)
	if s == "[]()" || s == "![]()" || s == "()" {
		return ""
	}
	return strings.Join(strings.Fields(s), " ")
}

func loadAnyImage(ref string) (image.Image, error) {
	switch {
	case strings.HasPrefix(ref, "devzat-file:"):
		b, err := loadUpload(strings.TrimPrefix(ref, "devzat-file:"))
		if err != nil {
			return nil, fmt.Errorf("missing image")
		}
		img, _, ok := decodeImageBytes(b)
		if !ok {
			return nil, fmt.Errorf("could not decode image")
		}
		return img, nil
	case strings.HasPrefix(ref, "data:image/"):
		raw, _, ok := decodeDataURI(ref)
		if !ok {
			return nil, fmt.Errorf("bad image data")
		}
		img, _, ok := decodeImageBytes(raw)
		if !ok {
			return nil, fmt.Errorf("could not decode image")
		}
		return img, nil
	case strings.HasPrefix(ref, "http://"), strings.HasPrefix(ref, "https://"):
		return fetchRemoteImage(ref)
	default:
		return nil, fmt.Errorf("unsupported image")
	}
}

var safeHTTP = &http.Client{
	Timeout: 10 * time.Second,
	Transport: &http.Transport{
		Proxy:                  nil,
		DialContext:            (&net.Dialer{Timeout: 5 * time.Second, Control: refusePrivateAddr}).DialContext,
		TLSHandshakeTimeout:    5 * time.Second,
		ResponseHeaderTimeout:  5 * time.Second,
		MaxResponseHeaderBytes: 64 << 10,
	},
	CheckRedirect: func(req *http.Request, via []*http.Request) error {
		if len(via) >= 5 {
			return errors.New("too many redirects")
		}
		return nil
	},
}

var cgnatNet = &net.IPNet{IP: net.IPv4(100, 64, 0, 0), Mask: net.CIDRMask(10, 32)}

// refusePrivateAddr keeps image fetches off the server's own network, such as cloud metadata endpoints.
// It runs on the resolved address, so DNS tricks don't get around it.
func refusePrivateAddr(_, address string, _ syscall.RawConn) error {
	host, _, err := net.SplitHostPort(address)
	if err != nil {
		return err
	}
	ip := net.ParseIP(host)
	if ip == nil || ip.IsLoopback() || ip.IsPrivate() || ip.IsUnspecified() || ip.IsLinkLocalUnicast() ||
		ip.IsLinkLocalMulticast() || ip.IsInterfaceLocalMulticast() || ip.IsMulticast() || cgnatNet.Contains(ip) {
		return errors.New("refusing to fetch from a private address")
	}
	return nil
}

func fetchRemoteBytes(ref string, limit int64) ([]byte, error) {
	req, err := http.NewRequest(http.MethodGet, ref, nil)
	if err != nil {
		return nil, fmt.Errorf("error fetching image")
	}
	req.Header.Set("User-Agent", "devzat")
	res, err := safeHTTP.Do(req)
	if err != nil {
		return nil, fmt.Errorf("error fetching image")
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("http: %s", http.StatusText(res.StatusCode))
	}
	b, err := io.ReadAll(io.LimitReader(res.Body, limit+1))
	if err != nil {
		return nil, fmt.Errorf("error fetching image")
	}
	if int64(len(b)) > limit {
		return nil, fmt.Errorf("image too large")
	}
	return b, nil
}

func fetchRemoteImage(ref string) (image.Image, error) {
	b, err := fetchRemoteBytes(ref, 30*1024*1024)
	if err != nil {
		return nil, err
	}
	config, _, err := image.DecodeConfig(bytes.NewReader(b))
	if err != nil || config.Width > 4032*2 || config.Height > 3024*2 {
		return nil, fmt.Errorf("invalid or too large to render")
	}
	img, _, err := image.Decode(bytes.NewReader(b))
	if err != nil {
		return nil, fmt.Errorf("error decoding image")
	}
	return img, nil
}

func renderImage(u *User, img image.Image, hq bool) string {
	width := 80
	if u != nil && u.winWidth > 0 {
		width = u.winWidth
	}
	if !hq {
		w := width / 2
		if w < 8 {
			w = 8
		}
		return imgRender(img, w)
	}
	mode := "blocks"
	if u != nil {
		mode = u.effectiveGfx()
	}
	switch mode {
	case "kitty":
		return kittyRender(img)
	case "iterm":
		return itermRender(img)
	case "sixel":
		return sixelRender(img)
	default:
		if width < 8 {
			width = 8
		}
		return imgRender(img, width)
	}
}

func (u *User) effectiveGfx() string {
	if u == nil {
		return "blocks"
	}
	switch strings.ToLower(strings.TrimSpace(u.Gfx)) {
	case "kitty", "iterm", "sixel", "blocks":
		return strings.ToLower(strings.TrimSpace(u.Gfx))
	default:
		if u.gfxAuto != "" {
			return u.gfxAuto
		}
		return "blocks"
	}
}

func detectGfx(termName string, env []string) string {
	blob := strings.ToLower(termName)
	for _, e := range env {
		blob += "\n" + strings.ToLower(e)
	}
	switch {
	case strings.Contains(blob, "kitty"), strings.Contains(blob, "ghostty"), strings.Contains(blob, "wezterm"):
		return "kitty"
	case strings.Contains(blob, "iterm"):
		return "iterm"
	default:
		return "blocks"
	}
}

func kittyRender(img image.Image) string {
	img = imaging.Fit(img, 1280, 1280, imaging.Lanczos)
	var buf bytes.Buffer
	if err := pngEncode(&buf, img); err != nil {
		return imgRender(img, 80)
	}
	payload := base64.StdEncoding.EncodeToString(buf.Bytes())
	const chunk = 4096
	var b strings.Builder
	for i := 0; i < len(payload); i += chunk {
		end := i + chunk
		if end > len(payload) {
			end = len(payload)
		}
		more := 1
		if end == len(payload) {
			more = 0
		}
		if i == 0 {
			fmt.Fprintf(&b, "\x1b_Gf=100,a=T,t=d,m=%d;%s\x1b\\", more, payload[i:end])
		} else {
			fmt.Fprintf(&b, "\x1b_Gm=%d;%s\x1b\\", more, payload[i:end])
		}
	}
	return b.String()
}

func itermRender(img image.Image) string {
	img = imaging.Fit(img, 1280, 1280, imaging.Lanczos)
	var buf bytes.Buffer
	if err := pngEncode(&buf, img); err != nil {
		return imgRender(img, 80)
	}
	return "\x1b]1337;File=inline=1;preserveAspectRatio=1;width=auto:" + base64.StdEncoding.EncodeToString(buf.Bytes()) + "\a"
}

func pngEncode(w io.Writer, img image.Image) error {
	return png.Encode(w, img)
}

// sixelRender draws a high-resolution picture for terminals that understand
// sixel (xterm, foot, WezTerm, Windows Terminal, and others). Colors are
// quantized to 64 so the sequence stays reasonable.
func sixelRender(img image.Image) string {
	img = imaging.Fit(img, 800, 800, imaging.Lanczos)
	b := img.Bounds()
	w, h := b.Dx(), b.Dy()
	if w < 1 || h < 1 {
		return ""
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "\x1bPq\"1;1;%d;%d", w, h)
	for i := 0; i < 64; i++ {
		r := (i >> 4) & 3
		g := (i >> 2) & 3
		bl := i & 3
		fmt.Fprintf(&sb, "#%d;2;%d;%d;%d", i, r*100/3, g*100/3, bl*100/3)
	}
	for y := 0; y < h; y += 6 {
		first := true
		for c := 0; c < 64; c++ {
			row := make([]byte, w)
			any := false
			for x := 0; x < w; x++ {
				var bits byte
				for dy := 0; dy < 6; dy++ {
					if y+dy >= h {
						break
					}
					rr, gg, bb, _ := img.At(b.Min.X+x, b.Min.Y+y+dy).RGBA()
					idx := int(rr>>14)<<4 | int(gg>>14)<<2 | int(bb>>14)
					if idx == c {
						bits |= 1 << dy
						any = true
					}
				}
				row[x] = bits + 63
			}
			if !any {
				continue
			}
			if !first {
				sb.WriteByte('$')
			}
			first = false
			fmt.Fprintf(&sb, "#%d%s", c, compressSixel(row))
		}
		sb.WriteByte('-')
	}
	sb.WriteString("\x1b\\")
	return sb.String()
}

func compressSixel(row []byte) string {
	var b strings.Builder
	for i := 0; i < len(row); {
		j := i + 1
		for j < len(row) && row[j] == row[i] && j-i < 255 {
			j++
		}
		if j-i >= 3 {
			fmt.Fprintf(&b, "!%d%c", j-i, row[i])
		} else {
			b.Write(row[i:j])
		}
		i = j
	}
	return b.String()
}
