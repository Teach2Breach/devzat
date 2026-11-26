//go:build live

package main

import (
	"bytes"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/acarl005/stripansi"
	gossh "golang.org/x/crypto/ssh"
)

func TestLiveSSH(t *testing.T) {
	port := freePort(t)
	alt := freePort(t)
	prof := freePort(t)
	dir := t.TempDir()
	data := filepath.Join(dir, "data")
	if err := os.MkdirAll(data, 0755); err != nil {
		t.Fatal(err)
	}
	cfg := filepath.Join(dir, "devzat.yml")
	key := filepath.Join(dir, "hostkey")
	if err := os.WriteFile(cfg, []byte(fmt.Sprintf(
		"port: %d\nalt_port: %d\nprofile_port: %d\nscrollback: 16\nhistory_hours: 24\ndata_dir: %s\nkey_file: %s\n",
		port, alt, prof, data, key)), 0644); err != nil {
		t.Fatal(err)
	}
	bin := filepath.Join(dir, "devzat")
	build := exec.Command("go", "build", "-o", bin, ".")
	build.Dir = "."
	build.Env = os.Environ()
	if out, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build: %v\n%s", err, out)
	}

	logPath := filepath.Join(dir, "server.log")
	logf, err := os.Create(logPath)
	if err != nil {
		t.Fatal(err)
	}
	defer logf.Close()

	addr := fmt.Sprintf("127.0.0.1:%d", port)
	var proc *exec.Cmd
	start := func() {
		t.Helper()
		if proc != nil && proc.Process != nil {
			_ = proc.Process.Kill()
			_, _ = proc.Process.Wait()
		}
		proc = exec.Command(bin)
		proc.Env = append(os.Environ(), "DEVZAT_CONFIG="+cfg)
		proc.Dir = dir
		proc.Stdout = logf
		proc.Stderr = logf
		if err := proc.Start(); err != nil {
			t.Fatal(err)
		}
		if err := waitPort(addr, 20*time.Second); err != nil {
			t.Fatalf("%v\n%s", err, logTail(logPath))
		}
	}
	stop := func() {
		if proc != nil && proc.Process != nil {
			_ = proc.Process.Kill()
			_, _ = proc.Process.Wait()
			proc = nil
		}
	}
	t.Cleanup(stop)
	start()

	signer := testSigner(t)
	bob := dialChat(t, addr, "bob", signer)
	defer bob.Close()
	bob.waitText(t, "Welcome to the chat", logPath)

	aliceKey := testSigner(t)
	alice := dialChat(t, addr, "alice", aliceKey)
	alice.waitText(t, "Welcome to the chat", logPath)

	marker := "local-history-marker"
	alice.send(t, marker)
	bob.waitText(t, marker, logPath)
	if !stampRe.MatchString(bob.text()) {
		t.Fatalf("bob did not see a timestamp:\n%s", bob.text())
	}

	alice.Close()
	bob.waitText(t, "has left the chat", logPath)

	alice = dialChat(t, addr, "alice", aliceKey)
	alice.waitText(t, "last 24h", logPath)
	alice.waitText(t, marker, logPath)

	stop()
	start()

	alice.Close()
	alice = dialChat(t, addr, "alice", aliceKey)
	defer alice.Close()
	alice.waitText(t, marker, logPath)
	if !strings.Contains(alice.text(), "last 24h") {
		t.Fatalf("history did not survive restart:\n%s\n%s", alice.text(), logTail(logPath))
	}

	pngBytes := livePNG()
	b64 := base64.StdEncoding.EncodeToString(pngBytes)
	before := len(alice.raw())
	alice.paste(t, b64, false)
	alice.waitRawSince(t, before, "\x1b[38;2;", logPath)

	alice.send(t, "gfx kitty")
	alice.waitText(t, "effective kitty", logPath)
	before = len(alice.raw())
	alice.paste(t, b64, true)
	alice.waitRawSince(t, before, "\x1b_Gf=100", logPath)
}

var stampRe = regexp.MustCompile(`\d{1,2}:\d{2} [AP]M`)

type chat struct {
	t      *testing.T
	client *gossh.Client
	sess   *gossh.Session
	in     io.WriteCloser
	mu     sync.Mutex
	buf    bytes.Buffer
}

func dialChat(t *testing.T, addr, user string, signer gossh.Signer) *chat {
	t.Helper()
	cfg := &gossh.ClientConfig{
		User:            user,
		Auth:            []gossh.AuthMethod{gossh.PublicKeys(signer)},
		HostKeyCallback: gossh.InsecureIgnoreHostKey(),
		Timeout:         5 * time.Second,
	}
	var client *gossh.Client
	var err error
	deadline := time.Now().Add(10 * time.Second)
	for {
		client, err = gossh.Dial("tcp", addr, cfg)
		if err == nil || time.Now().After(deadline) {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}
	if err != nil {
		t.Fatalf("dial %s: %v", user, err)
	}
	sess, err := client.NewSession()
	if err != nil {
		client.Close()
		t.Fatal(err)
	}
	if err := sess.RequestPty("xterm-256color", 40, 120, gossh.TerminalModes{
		gossh.ECHO:          1,
		gossh.TTY_OP_ISPEED: 14400,
		gossh.TTY_OP_OSPEED: 14400,
	}); err != nil {
		sess.Close()
		client.Close()
		t.Fatal(err)
	}
	stdout, err := sess.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	stderr, err := sess.StderrPipe()
	if err != nil {
		t.Fatal(err)
	}
	stdin, err := sess.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := sess.Shell(); err != nil {
		t.Fatal(err)
	}
	c := &chat{t: t, client: client, sess: sess, in: stdin}
	go func() { _, _ = io.Copy(c, stdout) }()
	go func() { _, _ = io.Copy(c, stderr) }()
	return c
}

func (c *chat) Write(p []byte) (int, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.buf.Write(p)
}

func (c *chat) raw() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.buf.String()
}

func (c *chat) text() string {
	return stripansi.Strip(c.raw())
}

func (c *chat) send(t *testing.T, line string) {
	t.Helper()
	if _, err := c.in.Write([]byte(line + "\r")); err != nil {
		t.Fatal(err)
	}
}

func (c *chat) paste(t *testing.T, b64 string, hq bool) {
	t.Helper()
	var b strings.Builder
	if hq {
		b.WriteString("!hq ")
	}
	b.WriteString("\x1b[200~data:image/png;base64,")
	b.WriteString(b64)
	b.WriteString("\x1b[201~")
	if _, err := c.in.Write([]byte(b.String())); err != nil {
		t.Fatal(err)
	}
}

func (c *chat) waitText(t *testing.T, sub, logPath string) {
	t.Helper()
	c.wait(t, logPath, func() bool { return strings.Contains(c.text(), sub) }, "text "+sub)
}

func (c *chat) waitRawSince(t *testing.T, start int, sub, logPath string) {
	t.Helper()
	c.wait(t, logPath, func() bool {
		raw := c.raw()
		if len(raw) < start {
			return false
		}
		return strings.Contains(raw[start:], sub)
	}, "bytes "+sub)
}

func (c *chat) wait(t *testing.T, logPath string, ok func() bool, what string) {
	t.Helper()
	deadline := time.Now().Add(8 * time.Second)
	for time.Now().Before(deadline) {
		if ok() {
			return
		}
		time.Sleep(50 * time.Millisecond)
	}
	t.Fatalf("timed out waiting for %s\n--- chat ---\n%s\n--- server ---\n%s", what, tail(c.text(), 2500), logTail(logPath))
}

func (c *chat) Close() {
	if c == nil {
		return
	}
	_ = c.in.Close()
	_ = c.sess.Close()
	_ = c.client.Close()
}

func testSigner(t *testing.T) gossh.Signer {
	t.Helper()
	_, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	signer, err := gossh.NewSignerFromKey(priv)
	if err != nil {
		t.Fatal(err)
	}
	return signer
}

func freePort(t *testing.T) int {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()
	return ln.Addr().(*net.TCPAddr).Port
}

func waitPort(addr string, d time.Duration) error {
	deadline := time.Now().Add(d)
	var last error
	for time.Now().Before(deadline) {
		conn, err := net.DialTimeout("tcp", addr, 200*time.Millisecond)
		if err == nil {
			conn.Close()
			return nil
		}
		last = err
		time.Sleep(100 * time.Millisecond)
	}
	if last == nil {
		last = fmt.Errorf("timeout")
	}
	return fmt.Errorf("%s not accepting: %w", addr, last)
}

func livePNG() []byte {
	img := image.NewRGBA(image.Rect(0, 0, 8, 8))
	for y := 0; y < 8; y++ {
		for x := 0; x < 8; x++ {
			img.Set(x, y, color.RGBA{uint8(20 + x*20), uint8(10 + y*20), 200, 255})
		}
	}
	var buf bytes.Buffer
	_ = png.Encode(&buf, img)
	return buf.Bytes()
}

func logTail(path string) string {
	b, err := os.ReadFile(path)
	if err != nil {
		return err.Error()
	}
	return tail(string(b), 2500)
}

func tail(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[len(s)-n:]
}
