package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// historyEntry is one room message kept so people can catch up after
// disconnecting. Direct messages are not stored.
type historyEntry struct {
	Room      string    `json:"room"`
	Sender    string    `json:"sender,omitempty"` // display name, ANSI color included
	Text      string    `json:"text"`
	Timestamp time.Time `json:"ts"`
}

var (
	historyMu      sync.Mutex
	history        []historyEntry
	historyWrites  int
	historyHardCap = 5000
)

func historyPath() string {
	return filepath.Join(Config.DataDir, "history.jsonl")
}

func loadHistory() {
	f, err := os.Open(historyPath())
	if err != nil {
		if !os.IsNotExist(err) {
			Log.Println("history:", err)
		}
		return
	}
	defer f.Close()

	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 64*1024), 1024*1024)
	loaded := make([]historyEntry, 0, 64)
	for sc.Scan() {
		var e historyEntry
		if err := json.Unmarshal(sc.Bytes(), &e); err != nil || e.Text == "" || e.Timestamp.IsZero() {
			continue
		}
		loaded = append(loaded, e)
	}
	if err := sc.Err(); err != nil {
		Log.Println("history:", err)
	}

	historyMu.Lock()
	history = loaded
	pruneHistoryLocked()
	rewriteHistoryFileLocked()
	historyMu.Unlock()
}

func recordHistory(room string, msg Message) {
	if msg.text == "" || room == "" {
		return
	}
	// Private messages are delivered directly and are not room history.
	if msg.messageType == PrivateMessageSend || msg.messageType == PrivateMessageReceive {
		return
	}
	ts := msg.timestamp
	if ts.IsZero() {
		ts = time.Now()
	}
	e := historyEntry{
		Room:      room,
		Text:      msg.text,
		Timestamp: ts,
	}
	if msg.sender != nil && msg.messageType != NoSenderMessage {
		e.Sender = msg.sender.Name
	}

	historyMu.Lock()
	defer historyMu.Unlock()
	history = append(history, e)
	pruneHistoryLocked()
	if !historyContainsLocked(e) {
		return
	}
	if err := appendHistoryLineLocked(e); err != nil {
		Log.Println("history:", err)
		return
	}
	historyWrites++
	if historyWrites%100 == 0 {
		rewriteHistoryFileLocked()
	}
}

func historyContainsLocked(e historyEntry) bool {
	for i := range history {
		if history[i].Room == e.Room && history[i].Text == e.Text && history[i].Timestamp.Equal(e.Timestamp) && history[i].Sender == e.Sender {
			return true
		}
	}
	return false
}

func pruneHistoryLocked() {
	if Config.HistoryHours <= 0 {
		maxN := Config.Scrollback
		if maxN <= 0 {
			maxN = 16
		}
		if len(history) > maxN {
			history = append([]historyEntry(nil), history[len(history)-maxN:]...)
		}
		return
	}
	cut := time.Now().Add(-time.Duration(Config.HistoryHours) * time.Hour)
	kept := history[:0]
	for _, e := range history {
		if !e.Timestamp.Before(cut) {
			kept = append(kept, e)
		}
	}
	history = kept
	if len(history) > historyHardCap {
		history = append([]historyEntry(nil), history[len(history)-historyHardCap:]...)
	}
}

func appendHistoryLineLocked(e historyEntry) error {
	if err := os.MkdirAll(Config.DataDir, 0755); err != nil {
		return err
	}
	f, err := os.OpenFile(historyPath(), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
	if err != nil {
		return err
	}
	defer f.Close()
	return json.NewEncoder(f).Encode(e)
}

func rewriteHistoryFileLocked() {
	if err := os.MkdirAll(Config.DataDir, 0755); err != nil {
		Log.Println("history:", err)
		return
	}
	tmp := historyPath() + ".tmp"
	f, err := os.Create(tmp)
	if err != nil {
		Log.Println("history:", err)
		return
	}
	enc := json.NewEncoder(f)
	for i := range history {
		if err := enc.Encode(history[i]); err != nil {
			f.Close()
			Log.Println("history:", err)
			return
		}
	}
	if err := f.Close(); err != nil {
		Log.Println("history:", err)
		return
	}
	if err := os.Rename(tmp, historyPath()); err != nil {
		Log.Println("history:", err)
	}
}

func compactHistory() {
	historyMu.Lock()
	defer historyMu.Unlock()
	pruneHistoryLocked()
	rewriteHistoryFileLocked()
}

func historyLabel() string {
	if Config.HistoryHours <= 0 {
		return "recent messages"
	}
	return fmt.Sprintf("last %dh", Config.HistoryHours)
}

func roomHasHistory(room string) bool {
	historyMu.Lock()
	defer historyMu.Unlock()
	pruneHistoryLocked()
	for _, e := range history {
		if e.Room == room {
			return true
		}
	}
	return false
}

func replayHistory(u *User, room string) {
	if u == nil || u.term == nil {
		return
	}
	historyMu.Lock()
	pruneHistoryLocked()
	entries := make([]historyEntry, 0)
	for _, e := range history {
		if e.Room == room {
			entries = append(entries, e)
		}
	}
	historyMu.Unlock()
	if len(entries) == 0 {
		return
	}
	if u.gui != nil {
		u.gui.send(guiHistoryMark{T: "history", Phase: "begin", Room: room, Label: historyLabel()})
		for _, e := range entries {
			msg := entryToMessage(e)
			msg.replay = true
			u.writeln(msg)
		}
		u.gui.send(guiHistoryMark{T: "history", Phase: "end", Room: room, Label: historyLabel()})
		return
	}
	_, _ = u.term.Write([]byte("\x1b[38;5;245m——— " + historyLabel() + " ———\x1b[0m\n"))
	for _, e := range entries {
		u.writeln(entryToMessage(e))
	}
	_, _ = u.term.Write([]byte("\x1b[38;5;245m——— now ———\x1b[0m\n"))
}

func entryToMessage(e historyEntry) Message {
	msg := Message{text: e.Text, timestamp: e.Timestamp, messageType: NoSenderMessage}
	if e.Sender != "" {
		msg.sender = &User{Name: e.Sender, ColorBG: "bg-off"}
		msg.messageType = DefaultMessage
	}
	return msg
}

func fmtMsgTime(u *User, t time.Time) string {
	if t.IsZero() {
		t = time.Now()
	}
	if u != nil && u.RelativeTime {
		return fmtRelativeTime(t)
	}
	loc := time.Local
	if u != nil && u.Timezone.Location != nil {
		loc = u.Timezone.Location
	}
	t = t.In(loc)
	now := time.Now().In(loc)
	var clock string
	if u != nil && u.FormatTime24 {
		clock = t.Format("15:04")
	} else {
		clock = t.Format("3:04 PM")
	}
	if t.Year() == now.Year() && t.YearDay() == now.YearDay() {
		return clock
	}
	if t.Year() == now.Year() {
		return t.Format("Jan 2") + " " + clock
	}
	return t.Format("2006 Jan 2") + " " + clock
}

func fmtRelativeTime(t time.Time) string {
	d := time.Since(t)
	future := false
	if d < 0 {
		d = -d
		future = true
	}
	var s string
	switch {
	case d < time.Minute:
		s = "now"
	case d < time.Hour:
		s = fmt.Sprintf("%dm", int(d.Minutes()))
	case d < 48*time.Hour:
		s = fmt.Sprintf("%dh", int(d.Hours()))
	default:
		s = fmt.Sprintf("%dd", int(d.Hours()/24))
	}
	if s == "now" {
		return s
	}
	if future {
		return s + " from now"
	}
	return s + " ago"
}
