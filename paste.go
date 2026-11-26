package main

import (
	"bytes"
	"crypto/rand"
	"encoding/hex"
	"io"
	"regexp"
	"strings"
	"sync"
)

var (
	bracketStart = []byte("\x1b[200~")
	bracketEnd   = []byte("\x1b[201~")
	itermStart   = []byte("\x1b]1337;File=")
	imgTokenRe   = regexp.MustCompile(`\[\[img:([0-9a-f]{16})\]\]`)
	bareHQRe     = regexp.MustCompile(`(^|\s)!hq(\s|$)`)
)

const maxPasteHold = 8 << 20

type storedPaste struct {
	data []byte
	ext  string
}

// imagePasteGate sits in front of the terminal reader. Bracketed pastes and
// iTerm inline-image sequences that contain a picture are swapped for a short
// token so the line editor never has to echo a multi-megabyte blob.
type imagePasteGate struct {
	r       io.Reader
	mu      sync.Mutex
	pending []byte
	hold    []byte
	images  map[string]storedPaste
	eof     bool
}

func newImagePasteGate(r io.Reader) *imagePasteGate {
	return &imagePasteGate{r: r, images: map[string]storedPaste{}}
}

func (g *imagePasteGate) Read(p []byte) (int, error) {
	g.mu.Lock()
	defer g.mu.Unlock()
	for len(g.pending) == 0 {
		if err := g.pump(); err != nil {
			if len(g.pending) > 0 {
				break
			}
			return 0, err
		}
	}
	n := copy(p, g.pending)
	g.pending = g.pending[n:]
	return n, nil
}

func (g *imagePasteGate) pump() error {
	for {
		if len(g.hold) == 0 {
			if err := g.mustRead(); err != nil {
				return err
			}
			continue
		}
		if idx := bytes.Index(g.hold, bracketStart); idx >= 0 {
			if idx > 0 {
				g.pending = append(g.pending, g.hold[:idx]...)
				g.hold = g.hold[idx:]
				return nil
			}
			endRel := bytes.Index(g.hold, bracketEnd)
			if endRel < 0 {
				if len(g.hold) >= maxPasteHold || g.eof {
					g.pending = append(g.pending, g.hold...)
					g.hold = nil
					return nil
				}
				if err := g.mustRead(); err != nil {
					if err == io.EOF {
						g.pending = append(g.pending, g.hold...)
						g.hold = nil
						if len(g.pending) == 0 {
							return io.EOF
						}
						return nil
					}
					return err
				}
				continue
			}
			body := append([]byte(nil), g.hold[len(bracketStart):endRel]...)
			g.hold = g.hold[endRel+len(bracketEnd):]
			if repl, ok := g.rewritePaste(body); ok {
				g.pending = append(g.pending, repl...)
			} else {
				g.pending = append(g.pending, bracketStart...)
				g.pending = append(g.pending, body...)
				g.pending = append(g.pending, bracketEnd...)
			}
			return nil
		}
		if idx := bytes.Index(g.hold, itermStart); idx >= 0 {
			if idx > 0 {
				g.pending = append(g.pending, g.hold[:idx]...)
				g.hold = g.hold[idx:]
				return nil
			}
			end, endLen := itermEnd(g.hold)
			if end < 0 {
				if len(g.hold) >= maxPasteHold || g.eof {
					g.pending = append(g.pending, g.hold...)
					g.hold = nil
					return nil
				}
				if err := g.mustRead(); err != nil {
					if err == io.EOF {
						g.pending = append(g.pending, g.hold...)
						g.hold = nil
						if len(g.pending) == 0 {
							return io.EOF
						}
						return nil
					}
					return err
				}
				continue
			}
			body := append([]byte(nil), g.hold[:end+endLen]...)
			g.hold = g.hold[end+endLen:]
			if repl, ok := g.rewritePaste(body); ok {
				g.pending = append(g.pending, repl...)
			} else {
				g.pending = append(g.pending, body...)
			}
			return nil
		}

		keep := suffixIsPrefix(g.hold, bracketStart, itermStart)
		if keep == len(g.hold) {
			if g.eof {
				g.pending = append(g.pending, g.hold...)
				g.hold = nil
				return nil
			}
			if err := g.mustRead(); err != nil {
				if err == io.EOF {
					g.pending = append(g.pending, g.hold...)
					g.hold = nil
					if len(g.pending) == 0 {
						return io.EOF
					}
					return nil
				}
				return err
			}
			continue
		}
		g.pending = append(g.pending, g.hold[:len(g.hold)-keep]...)
		g.hold = g.hold[len(g.hold)-keep:]
		return nil
	}
}

func itermEnd(b []byte) (int, int) {
	bel := bytes.IndexByte(b, 0x07)
	st := bytes.Index(b, []byte("\x1b\\"))
	if bel >= 0 && (st < 0 || bel < st) {
		return bel, 1
	}
	if st >= 0 {
		return st, 2
	}
	return -1, 0
}

func (g *imagePasteGate) mustRead() error {
	if g.eof {
		return io.EOF
	}
	var tmp [8192]byte
	n, err := g.r.Read(tmp[:])
	if n > 0 {
		g.hold = append(g.hold, tmp[:n]...)
	}
	if err == io.EOF {
		g.eof = true
	}
	if n > 0 {
		return nil
	}
	if err != nil {
		return err
	}
	g.eof = true
	return io.EOF
}

func suffixIsPrefix(buf []byte, markers ...[]byte) int {
	max := 0
	for _, m := range markers {
		limit := len(m) - 1
		if limit > len(buf) {
			limit = len(buf)
		}
		for n := limit; n > 0; n-- {
			if bytes.HasPrefix(m, buf[len(buf)-n:]) && n > max {
				max = n
			}
		}
	}
	return max
}

func (g *imagePasteGate) rewritePaste(body []byte) ([]byte, bool) {
	caption, imgs, hq, ok := extractPastedImages(body)
	if !ok || len(imgs) == 0 {
		return nil, false
	}
	var b strings.Builder
	if hq {
		b.WriteString("!hq ")
	}
	b.WriteString(caption)
	for _, img := range imgs {
		if b.Len() > 0 && !strings.HasSuffix(b.String(), " ") {
			b.WriteByte(' ')
		}
		id := newPasteID()
		g.images[id] = storedPaste{data: img.data, ext: img.ext}
		b.WriteString("[[img:" + id + "]]")
	}
	b.WriteByte('\r')
	return []byte(b.String()), true
}

func newPasteID() string {
	var buf [8]byte
	if _, err := rand.Read(buf[:]); err != nil {
		return "0000000000000000"
	}
	return hex.EncodeToString(buf[:])
}

// expand turns image tokens into markdown the rest of the server already
// knows how to draw. A bare "!hq" next to a token asks for the sharp renderer.
func (g *imagePasteGate) expand(line string) string {
	if g == nil || !strings.Contains(line, "[[img:") {
		return line
	}
	hq := bareHQRe.MatchString(line)
	if hq {
		line = strings.TrimSpace(bareHQRe.ReplaceAllString(line, " "))
	}
	line = imgTokenRe.ReplaceAllStringFunc(line, func(m string) string {
		id := imgTokenRe.FindStringSubmatch(m)[1]
		img, ok := g.take(id)
		if !ok {
			return ""
		}
		name, err := saveUpload(img.data, img.ext)
		if err != nil {
			Log.Println("image:", err)
			return "(image failed to save)"
		}
		if hq {
			return "!hq[](devzat-file:" + name + ")"
		}
		return "![](devzat-file:" + name + ")"
	})
	return strings.TrimSpace(line)
}

func (g *imagePasteGate) take(id string) (storedPaste, bool) {
	g.mu.Lock()
	defer g.mu.Unlock()
	img, ok := g.images[id]
	if ok {
		delete(g.images, id)
	}
	return img, ok
}
