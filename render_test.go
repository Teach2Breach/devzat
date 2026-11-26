package main

import (
	"bytes"
	"io"
	"strings"
	"testing"
	"time"

	"github.com/acarl005/stripansi"
	"github.com/quackduck/term"
)

func TestLineBreakIsCorrect(t *testing.T) {
	buff := bytes.NewBuffer(nil)
	terminal := term.NewTerminal(buff, "")
	_ = terminal.SetSize(10000, 10000) // disable any formatting done by term
	user := &User{
		term:     terminal,
		winWidth: 80,
	}
	tests := []struct {
		name  string
		input Message
		want  string
	}{
		{
			name: "SimpleMessage",
			input: NewFakeUserMessage(
				"Test",
				"Testing",
				makeDummyRoom(),
			),
			want: "15:04 Test: Testing\r\n",
		},
		{
			name: "SimpleMessageLineWrap",
			input: NewFakeUserMessage(
				"Test",
				"Testing With A Long String That Is Over 80 chars long even excluding the username",
				makeDummyRoom(),
			),
			want: "15:04 Test: Testing With A Long String That Is Over 80 chars long even excluding\r\n            the username\r\n",
		},
		{
			name: "TestLineBreakIsCorrectOfByOne",
			input: NewFakeUserMessage(
				"Arkaeriit",
				"Maybe there is a off-by-one error in the way the line width is counted.",
				makeDummyRoom(),
			),
			want: "15:04 Arkaeriit: Maybe there is a off-by-one error in the way the line width is\r\n                 counted.\r\n",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			user.FormatTime24 = true
			user.Timezone = tz{time.UTC}
			msg := test.input
			msg.timestamp = time.Now().UTC()
			user.writeln(msg)
			read, err := io.ReadAll(buff)
			if err != nil {
				panic(err)
			}
			received := removeSpaceBeforeLineBreak(stripansi.Strip(string(read)))
			want := strings.Replace(test.want, "15:04", fmtMsgTime(user, msg.timestamp), 1)
			if received != want {
				t.Errorf("got %q, want %q", received, want)
			}
		})
	}
}

func removeSpaceBeforeLineBreak(s string) string {
	oldLines := strings.Split(s, "\r\n")
	lines := make([]string, 0, len(oldLines))
	for _, line := range oldLines {
		lines = append(lines, strings.TrimRight(line, " "))
	}

	return strings.Join(lines, "\r\n")
}
