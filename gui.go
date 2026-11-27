package main

import (
	"bufio"
	"bytes"
	"encoding/base64"
	"encoding/json"
	"errors"
	"image"
	"image/jpeg"
	"io"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/acarl005/stripansi"
	"github.com/disintegration/imaging"
	"github.com/gliderlabs/ssh"
)

// Desktop clients request this SSH subsystem and exchange newline-delimited
// JSON instead of terminal output. They still log in with an SSH key, so a
// person has the same ID, name, and color in the desktop app and the terminal.
const (
	guiSubsystem = "devzat-json"
	guiProtocol  = 1
	guiMaxLine   = 12 << 20
	guiMaxBlob   = 6 << 20
	guiMaxPixels = 50_000_000
	// guiQuiet starts internal lines from the desktop protocol. Typed input never contains it.
	guiQuiet = "\uE000"
)

var errLineTooLong = errors.New("line too long")

type guiSink struct {
	mu   sync.Mutex
	w    io.Writer
	dead bool
}

func (g *guiSink) send(v any) {
	b, err := json.Marshal(v)
	if err != nil {
		Log.Println("gui:", err)
		return
	}
	g.write(append(b, '\n'))
}

func (g *guiSink) write(b []byte) {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.dead {
		return
	}
	if _, err := g.w.Write(b); err != nil {
		g.dead = true
	}
}

type guiSimple struct {
	T string `json:"t"`
}

type guiBye struct {
	T      string `json:"t"`
	Reason string `json:"reason"`
}

type guiHistoryMark struct {
	T     string `json:"t"`
	Phase string `json:"phase"`
	Room  string `json:"room"`
	Label string `json:"label"`
}

type guiMsg struct {
	T         string    `json:"t"`
	Kind      string    `json:"kind"` // chat, bot, system, dm_in, dm_out
	Room      string    `json:"room"`
	From      string    `json:"from,omitempty"` // display name with ANSI color; for dm_out it is the recipient
	FromPlain string    `json:"from_plain,omitempty"`
	Text      string    `json:"text"` // markdown, may contain ANSI color codes
	TS        time.Time `json:"ts"`
	Mention   bool      `json:"mention,omitempty"`
	History   bool      `json:"history,omitempty"`
}

type guiWelcome struct {
	T            string `json:"t"`
	Protocol     int    `json:"protocol"`
	ID           string `json:"id"`
	Name         string `json:"name"`
	Plain        string `json:"plain"`
	Admin        bool   `json:"admin"`
	HistoryHours int    `json:"history_hours"`
	MaxUpload    int    `json:"max_upload"`
}

type guiRoomInfo struct {
	Name  string `json:"name"`
	Count int    `json:"count"`
}

type guiUserInfo struct {
	Name  string `json:"name"`
	Plain string `json:"plain"`
	You   bool   `json:"you,omitempty"`
	Admin bool   `json:"admin,omitempty"`
	Idle  bool   `json:"idle,omitempty"`
}

type guiState struct {
	T     string        `json:"t"`
	Room  string        `json:"room"`
	DM    string        `json:"dm,omitempty"`
	Name  string        `json:"name"`
	Plain string        `json:"plain"`
	Rooms []guiRoomInfo `json:"rooms"`
	Users []guiUserInfo `json:"users"`
}

type guiAck struct {
	T     string `json:"t"`
	Req   int64  `json:"req"`
	OK    bool   `json:"ok"`
	Ref   string `json:"ref,omitempty"`
	Error string `json:"error,omitempty"`
}

type guiBlob struct {
	T     string `json:"t"`
	Req   int64  `json:"req"`
	Ref   string `json:"ref"`
	Mime  string `json:"mime,omitempty"`
	Data  string `json:"data,omitempty"`
	Error string `json:"error,omitempty"`
}

type guiRequest struct {
	T       string `json:"t"`
	Req     int64  `json:"req"`
	Text    string `json:"text"`
	Data    string `json:"data"`
	Caption string `json:"caption"`
	HQ      bool   `json:"hq"`
	To      string `json:"to"`
	Ref     string `json:"ref"`
	Room    string `json:"room"`
}

func (g *guiSink) message(u *User, msg Message) {
	text := strings.ReplaceAll(msg.text, `\n`, "\n")
	text = strings.ReplaceAll(text, `\`+"\n", `\n`)
	text = strings.ReplaceAll(text, "\a", "")
	ts := msg.timestamp
	if ts.IsZero() {
		ts = time.Now()
	}
	m := guiMsg{T: "msg", Room: u.room.name, Text: text, TS: ts, History: msg.replay}
	switch msg.messageType {
	case PrivateMessageSend:
		m.Kind = "dm_out"
	case PrivateMessageReceive:
		m.Kind = "dm_in"
	case NoSenderMessage:
		m.Kind = "system"
	default:
		m.Kind = "chat"
		if msg.sender != nil && msg.sender.Name == Devbot {
			m.Kind = "bot"
		}
	}
	if msg.sender != nil && msg.messageType != NoSenderMessage {
		m.From = msg.sender.Name
		m.FromPlain = stripansi.Strip(msg.sender.Name)
	}
	me := stripansi.Strip(u.Name)
	m.Mention = !msg.replay && msg.messageType != NoSenderMessage && m.FromPlain != me && strings.Contains(msg.text, u.Name)
	g.send(m)
}

func guiSnapshot(u *User) guiState {
	room := u.room
	st := guiState{T: "state", Room: room.name, Name: u.Name, Plain: stripansi.Strip(u.Name)}
	if peer := u.messaging; peer != nil {
		st.DM = stripansi.Strip(peer.Name)
	}
	rooms := roomList()
	sort.Slice(rooms, func(i, j int) bool {
		if rooms[i] == MainRoom || rooms[j] == MainRoom {
			return rooms[i] == MainRoom
		}
		return rooms[i].name < rooms[j].name
	})
	for _, r := range rooms {
		r.usersMutex.RLock()
		n := len(r.users)
		r.usersMutex.RUnlock()
		st.Rooms = append(st.Rooms, guiRoomInfo{Name: r.name, Count: n})
	}
	room.usersMutex.RLock()
	for _, other := range room.users {
		st.Users = append(st.Users, guiUserInfo{
			Name:  other.Name,
			Plain: stripansi.Strip(other.Name),
			You:   other == u,
			Admin: auth(other),
			Idle:  time.Since(other.lastInteract) > 10*time.Minute,
		})
	}
	room.usersMutex.RUnlock()
	sort.Slice(st.Users, func(i, j int) bool {
		return strings.ToLower(st.Users[i].Plain) < strings.ToLower(st.Users[j].Plain)
	})
	return st
}

func guiSession(s ssh.Session) {
	sink := &guiSink{w: s}
	inR, inW := io.Pipe()
	lines := make(chan string, 64)
	go func() { // the user's virtual terminal reads typed lines from here
		defer inW.Close()
		for l := range lines {
			if _, err := io.WriteString(inW, l+"\r"); err != nil {
				return
			}
		}
	}()
	go guiReadLoop(s, sink, lines)

	u := newGUIUser(s, sink, inR)
	if u == nil {
		s.Close()
		return
	}
	sink.send(guiWelcome{
		T:            "welcome",
		Protocol:     guiProtocol,
		ID:           u.id,
		Name:         u.Name,
		Plain:        stripansi.Strip(u.Name),
		Admin:        auth(u),
		HistoryHours: Config.HistoryHours,
		MaxUpload:    maxImageBytes,
	})
	go guiStateLoop(u, sink, s.Context().Done())
	defer protectFromPanic()
	u.repl()
}

func guiStateLoop(u *User, sink *guiSink, done <-chan struct{}) {
	tick := time.NewTicker(300 * time.Millisecond)
	defer tick.Stop()
	var last []byte
	for {
		if b, err := json.Marshal(guiSnapshot(u)); err == nil && !bytes.Equal(b, last) {
			last = b
			sink.write(append(append([]byte(nil), b...), '\n'))
		}
		select {
		case <-done:
			return
		case <-tick.C:
		}
	}
}

func guiReadLoop(s ssh.Session, sink *guiSink, lines chan<- string) {
	defer close(lines)
	r := bufio.NewReaderSize(s, 64<<10)
	fetches := make(chan struct{}, 3)
	for {
		raw, err := readLimitedLine(r, guiMaxLine)
		if errors.Is(err, errLineTooLong) {
			continue
		}
		if err != nil {
			return
		}
		var req guiRequest
		if json.Unmarshal(raw, &req) != nil {
			continue
		}
		switch req.T {
		case "send":
			if l := guiInputLine(req.Text); l != "" {
				lines <- l
			}
		case "join":
			room := strings.TrimSpace(req.Room)
			if !validGUIRoom(room) {
				sink.send(guiAck{T: "ack", Req: req.Req, Error: "room names start with # and have no spaces"})
				continue
			}
			lines <- guiQuiet + "join " + room
		case "upload":
			ref, err := guiUpload(req)
			if err != nil {
				sink.send(guiAck{T: "ack", Req: req.Req, Error: err.Error()})
				continue
			}
			tag := "!["
			if req.HQ {
				tag = "!hq["
			}
			line := tag + guiCaption(req.Caption) + "](" + ref + ")"
			if to := strings.TrimSpace(req.To); to != "" && !strings.ContainsAny(to, " \t") {
				line = "=" + to + " " + line
			}
			lines <- line
			sink.send(guiAck{T: "ack", Req: req.Req, OK: true, Ref: ref})
		case "fetch":
			fetches <- struct{}{}
			go func(req guiRequest) {
				defer func() { <-fetches }()
				data, mime, err := guiFetch(req.Ref)
				if err != nil {
					sink.send(guiBlob{T: "blob", Req: req.Req, Ref: req.Ref, Error: err.Error()})
					return
				}
				sink.send(guiBlob{T: "blob", Req: req.Req, Ref: req.Ref, Mime: mime, Data: base64.StdEncoding.EncodeToString(data)})
			}(req)
		case "ping":
			sink.send(guiAck{T: "pong", Req: req.Req, OK: true})
		}
	}
}

func readLimitedLine(r *bufio.Reader, max int) ([]byte, error) {
	var buf []byte
	for {
		chunk, err := r.ReadSlice('\n')
		if len(buf)+len(chunk) > max {
			for err == bufio.ErrBufferFull {
				_, err = r.ReadSlice('\n')
			}
			if err != nil {
				return nil, err
			}
			return nil, errLineTooLong
		}
		buf = append(buf, chunk...)
		if err == nil {
			return bytes.TrimRight(buf, "\r\n"), nil
		}
		if err != bufio.ErrBufferFull {
			return nil, err
		}
	}
}

// guiInputLine turns desktop input into one terminal line. Real newlines become devzat's \n, a typed
// backslash-n stays literal, and control characters that the line editor would treat as keys are dropped.
func guiInputLine(text string) string {
	text = strings.ReplaceAll(text, `\n`, `\\n`)
	text = strings.ReplaceAll(text, "\r\n", "\n")
	text = strings.ReplaceAll(text, "\n", `\n`)
	var b strings.Builder
	for _, r := range text {
		switch {
		case r == '\t':
			b.WriteString("    ")
		case r < 0x20, r == 0x7f, r >= 0x80 && r < 0xa0, r == []rune(guiQuiet)[0]:
		default:
			b.WriteRune(r)
		}
	}
	out := []rune(strings.TrimSpace(b.String()))
	if len(out) > maxMsgLen {
		out = out[:maxMsgLen]
	}
	return string(out)
}

func validGUIRoom(room string) bool {
	if !strings.HasPrefix(room, "#") || len(room) < 2 || len(room) > MaxRoomNameLen {
		return false
	}
	for _, r := range room {
		if r <= ' ' || r == 0x7f || r == []rune(guiQuiet)[0] {
			return false
		}
	}
	return true
}

// guiQuietCommand runs desktop actions that shouldn't echo into the room the way typed commands do.
func guiQuietCommand(u *User, cmd string) {
	verb, arg, _ := strings.Cut(cmd, " ")
	switch verb {
	case "join":
		if !validGUIRoom(arg) {
			return
		}
		u.messaging = nil
		if arg == MainRoom.name {
			u.changeRoom(MainRoom)
		} else {
			u.changeRoom(getOrCreateRoom(arg))
		}
		u.formatPrompt()
	}
}

func guiCaption(s string) string {
	s = strings.Map(func(r rune) rune {
		switch r {
		case '[', ']', '(', ')', '\n', '\r', '\t':
			return ' '
		}
		if r < 0x20 || r == 0x7f {
			return -1
		}
		return r
	}, s)
	s = strings.Join(strings.Fields(s), " ")
	if r := []rune(s); len(r) > 120 {
		s = string(r[:120])
	}
	return s
}

func guiUpload(req guiRequest) (string, error) {
	raw, err := base64.StdEncoding.DecodeString(req.Data)
	if err != nil {
		return "", errors.New("bad image data")
	}
	if len(raw) > maxImageBytes {
		return "", errors.New("image is larger than 8 MB")
	}
	_, format, ok := decodeImageBytes(raw)
	if !ok {
		return "", errors.New("not a supported image (png, jpg, gif, webp)")
	}
	name, err := saveUpload(raw, extForFormat(format))
	if err != nil {
		Log.Println("gui upload:", err)
		return "", errors.New("could not save image")
	}
	return "devzat-file:" + name, nil
}

// guiFetch returns image bytes for a message reference. Remote pictures are fetched by the server,
// so desktop users never connect to image hosts themselves.
func guiFetch(ref string) ([]byte, string, error) {
	var raw []byte
	switch {
	case strings.HasPrefix(ref, "devzat-file:"):
		b, err := loadUpload(strings.TrimPrefix(ref, "devzat-file:"))
		if err != nil {
			return nil, "", errors.New("image not found")
		}
		raw = b
	case strings.HasPrefix(ref, "https://"), strings.HasPrefix(ref, "http://"):
		b, err := fetchRemoteBytes(ref, 30<<20)
		if err != nil {
			return nil, "", err
		}
		raw = b
	default:
		return nil, "", errors.New("unsupported image reference")
	}
	cfg, format, err := image.DecodeConfig(bytes.NewReader(raw))
	if err != nil {
		return nil, "", errors.New("not an image")
	}
	if cfg.Width <= 0 || cfg.Height <= 0 || cfg.Width*cfg.Height > guiMaxPixels {
		return nil, "", errors.New("image is too large")
	}
	if len(raw) <= guiMaxBlob && cfg.Width <= 4096 && cfg.Height <= 4096 {
		return raw, "image/" + format, nil
	}
	img, _, err := image.Decode(bytes.NewReader(raw))
	if err != nil {
		return nil, "", errors.New("could not decode image")
	}
	img = imaging.Fit(img, 2048, 2048, imaging.Lanczos)
	var buf bytes.Buffer
	if err := jpeg.Encode(&buf, img, &jpeg.Options{Quality: 88}); err != nil {
		return nil, "", errors.New("could not shrink image")
	}
	return buf.Bytes(), "image/jpeg", nil
}
