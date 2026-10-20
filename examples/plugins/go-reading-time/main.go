// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Example CMS plugin in Go: stores an estimated reading time (minutes) of a
// text field into an Int field whenever an entry is created or updated.
//
// Config: {"sourceField": "body", "targetField": "readingTime", "wordsPerMinute": 200}
//
// Build (Go >= 1.24):
//
//	GOOS=wasip1 GOARCH=wasm go build -buildmode=c-shared -o reading-time.wasm .
package main

import (
	"math"
	"strings"

	"github.com/extism/go-pdk"
)

type input struct {
	Action      string         `json:"action"`
	ContentType string         `json:"contentType"`
	Data        map[string]any `json:"data"`
	Config      struct {
		SourceField    string  `json:"sourceField"`
		TargetField    string  `json:"targetField"`
		WordsPerMinute float64 `json:"wordsPerMinute"`
	} `json:"config"`
}

//go:wasmexport manifest
func manifest() int32 {
	pdk.OutputJSON(map[string]string{
		"version":     "1.0.0",
		"description": "Computes the reading time of a text field.",
	})
	return 0
}

//go:wasmexport before_write
func beforeWrite() int32 {
	var in input
	if err := pdk.InputJSON(&in); err != nil {
		pdk.SetError(err)
		return 1
	}
	if in.Action != "create" && in.Action != "update" {
		pdk.OutputString("{}")
		return 0
	}
	src, dst, wpm := in.Config.SourceField, in.Config.TargetField, in.Config.WordsPerMinute
	if src == "" {
		src = "body"
	}
	if dst == "" {
		dst = "readingTime"
	}
	if wpm <= 0 {
		wpm = 200
	}
	text, _ := in.Data[src].(string)
	words := len(strings.Fields(text))
	in.Data[dst] = int(math.Ceil(float64(words) / wpm))
	pdk.OutputJSON(map[string]any{"data": in.Data})
	return 0
}

func main() {}
