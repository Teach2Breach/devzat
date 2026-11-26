package main

import (
	"bytes"
	"encoding/base64"
	"image"
	"image/color"
	"image/png"
	"io"
	"os"
	"strings"
	"testing"
	"time"

	terminal "github.com/quackduck/term"
)

func tinyPNG(t *testing.T) []byte {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, 8, 8))
	for y := 0; y < 8; y++ {
		for x := 0; x < 8; x++ {
			img.Set(x, y, color.RGBA{uint8(x * 30), uint8(y * 30), 180, 255})
		}
	}
	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		t.Fatal(err)
	}
	return buf.Bytes()
}

func withDataDir(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	oldDir := Config.DataDir
	oldHours := Config.HistoryHours
	oldScroll := Config.Scrollback
	Config.DataDir = dir
	Config.HistoryHours = 24
	historyMu.Lock()
	history = nil
	historyWrites = 0
	historyMu.Unlock()
	t.Cleanup(func() {
		Config.DataDir = oldDir
		Config.HistoryHours = oldHours
		Config.Scrollback = oldScroll
		historyMu.Lock()
		history = nil
		historyWrites = 0
		historyMu.Unlock()
	})
	return dir
}

func TestHistoryKeepsADayAndDropsOlder(t *testing.T) {
	withDataDir(t)

	recent := NewMessage(&User{Name: "alice"}, "hello from the past")
	recent.timestamp = time.Now().Add(-time.Hour)
	recordHistory("#main", recent)

	stale := NewMessage(&User{Name: "alice"}, "too old to keep")
	stale.timestamp = time.Now().Add(-48 * time.Hour)
	recordHistory("#main", stale)

	historyMu.Lock()
	history = nil
	historyMu.Unlock()
	loadHistory()

	buf := &bytes.Buffer{}
	u := &User{
		term:         terminal.NewTerminal(buf, ""),
		winWidth:     80,
		Timezone:     tz{time.UTC},
		FormatTime24: true,
		room:         MainRoom,
	}
	replayHistory(u, "#main")
	got := buf.String()
	if !strings.Contains(got, "hello from the past") {
		t.Fatalf("missing recent history: %q", got)
	}
	if strings.Contains(got, "too old to keep") {
		t.Fatalf("kept expired history: %q", got)
	}
	if !strings.Contains(got, "last 24h") {
		t.Fatalf("missing history banner: %q", got)
	}
}

func TestMessageStampUsesTimezone(t *testing.T) {
	u := &User{Timezone: tz{time.UTC}, FormatTime24: true}
	got := fmtMsgTime(u, time.Date(2020, 1, 2, 15, 4, 0, 0, time.UTC))
	if !strings.Contains(got, "15:04") || !strings.Contains(got, "2020") {
		t.Fatalf("stamp = %q", got)
	}
	u.RelativeTime = true
	rel := fmtMsgTime(u, time.Now().Add(-2*time.Hour))
	if rel != "2h ago" {
		t.Fatalf("relative = %q", rel)
	}
}

func TestNormalizeLooseImageLinks(t *testing.T) {
	url := "https://cdn.britannica.com/34/235834-050-C5843610/two-different-breeds-of-cats-side-by-side-outdoors-in-the-garden.jpg"
	if got := normalizeImageLinks("!hq[]" + url); got != "!hq[]("+url+")" {
		t.Fatalf("missing parens: %s", got)
	}
	if got := normalizeImageLinks("!hq " + url); got != "!hq[]("+url+")" {
		t.Fatalf("!hq prefix: %s", got)
	}
	if got := normalizeImageLinks("!cat " + url); got != "![cat]("+url+")" {
		t.Fatalf("caption form: %s", got)
	}
	if got := normalizeImageLinks(url); got != "![]("+url+")" {
		t.Fatalf("bare link: %s", got)
	}
	kept := "![cat](" + url + ")"
	if got := normalizeImageLinks(kept); got != kept {
		t.Fatalf("rewrote existing image: %s", got)
	}
	if got := normalizeImageLinks("see https://example.com/cat.jpg please"); got != "see https://example.com/cat.jpg please" {
		t.Fatalf("help example was rewritten: %s", got)
	}
}

func TestBritannicaCatPhoto(t *testing.T) {
	img, err := loadAnyImage("https://cdn.britannica.com/34/235834-050-C5843610/two-different-breeds-of-cats-side-by-side-outdoors-in-the-garden.jpg")
	if err != nil {
		if strings.Contains(err.Error(), "fetch") {
			t.Skip(err)
		}
		t.Fatal(err)
	}
	if img.Bounds().Dx() < 32 || img.Bounds().Dy() < 32 {
		t.Fatalf("decoded photo is tiny: %v", img.Bounds())
	}
}

func TestAbsorbDataURIAndRenderBlocky(t *testing.T) {
	withDataDir(t)
	pngBytes := tinyPNG(t)
	uri := "data:image/png;base64," + base64.StdEncoding.EncodeToString(pngBytes)
	got := absorbEmbeddedImages(uri)
	if strings.Contains(got, "base64") || !strings.Contains(got, "devzat-file:") {
		t.Fatalf("absorb = %q", got)
	}

	buf := &bytes.Buffer{}
	u := &User{term: terminal.NewTerminal(buf, ""), winWidth: 40, Gfx: "blocks"}
	u.writeln(NewMessage(&User{Name: "alice"}, got))
	out := buf.String()
	if !strings.Contains(out, "\x1b[38;2;") {
		t.Fatalf("blocky image was not drawn: %q", out)
	}

	u.Gfx = "kitty"
	buf.Reset()
	u.writeln(NewMessage(&User{Name: "alice"}, strings.Replace(got, "![](", "!hq[](", 1)))
	out = buf.String()
	if !strings.Contains(out, "\x1b_Gf=100") {
		t.Fatalf("kitty image was not drawn: %q", out[:min(200, len(out))])
	}
}

func TestHQBlocksAreSharperThanDefault(t *testing.T) {
	img := image.NewRGBA(image.Rect(0, 0, 80, 40))
	for y := 0; y < 40; y++ {
		for x := 0; x < 80; x++ {
			img.Set(x, y, color.RGBA{uint8(x * 3), uint8(y * 6), 90, 255})
		}
	}
	u := &User{winWidth: 40, Gfx: "blocks"}
	low := renderImage(u, img, false)
	high := renderImage(u, img, true)
	if len(high) <= len(low) {
		t.Fatalf("expected the high-res block render to be larger (%d vs %d)", len(high), len(low))
	}
	if !strings.Contains(sixelRender(img), "\x1bPq") {
		t.Fatal("sixel render missing header")
	}
	if !strings.Contains(itermRender(img), "\x1b]1337;File=") {
		t.Fatal("iterm render missing header")
	}
}

func TestPasteGateSwapsImageForToken(t *testing.T) {
	pngBytes := tinyPNG(t)
	payload := "data:image/png;base64," + base64.StdEncoding.EncodeToString(pngBytes)
	raw := append([]byte("\x1b[200~"), payload...)
	raw = append(raw, []byte("\x1b[201~")...)

	r, w := io.Pipe()
	g := newImagePasteGate(r)
	go func() {
		_, _ = w.Write(raw)
		_ = w.Close()
	}()
	got, err := io.ReadAll(g)
	if err != nil {
		t.Fatal(err)
	}
	if bytes.Contains(got, []byte("data:image")) {
		t.Fatalf("image bytes leaked to the terminal: %q", got)
	}
	if !bytes.Contains(got, []byte("[[img:")) || !bytes.HasSuffix(bytes.TrimRight(got, "\n"), []byte("\r")) {
		t.Fatalf("gate output = %q", got)
	}

	// A normal paste is left alone so multiline text still works.
	text := []byte("\x1b[200~hello there\x1b[201~")
	r, w = io.Pipe()
	g = newImagePasteGate(r)
	go func() {
		_, _ = w.Write(text)
		_ = w.Close()
	}()
	got, err = io.ReadAll(g)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, text) {
		t.Fatalf("text paste changed: %q", got)
	}
}

func TestExpandPasteTokenSavesFile(t *testing.T) {
	withDataDir(t)
	g := newImagePasteGate(bytes.NewReader(nil))
	g.images["0123456789abcdef"] = storedPaste{data: tinyPNG(t), ext: ".png"}
	line := g.expand("!hq [[img:0123456789abcdef]]")
	if !strings.HasPrefix(line, "!hq[](devzat-file:") || !strings.HasSuffix(line, ".png)") {
		t.Fatalf("expand = %q", line)
	}
	name := strings.TrimPrefix(strings.TrimSuffix(line, ")"), "!hq[](devzat-file:")
	if _, err := os.Stat(Config.DataDir + "/uploads/" + name); err != nil {
		t.Fatal(err)
	}
	marked := markHQImages(line)
	if !strings.Contains(marked, "dzhq:devzat-file:") {
		t.Fatalf("markHQ = %q", marked)
	}
}
