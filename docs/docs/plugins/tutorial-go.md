---
sidebar_position: 5
title: "Tutorial: Go"
---

# Writing a plugin in Go

Go 1.24 and later can export functions to WebAssembly with
`//go:wasmexport`. This plugin (`examples/plugins/go-reading-time`, built
and tested by the project's test suite) stores the reading time of a text
field into an `INT` field.

## Setup

```bash
mkdir reading-time && cd reading-time
go mod init example.com/tess-plugins/reading-time
go get github.com/extism/go-pdk
```

## Code

`main.go`:

```go
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
```

## Build

```bash
GOOS=wasip1 GOARCH=wasm go build -buildmode=c-shared -o reading-time.wasm .
```

## Use it

1. Add an `INT` field `readingTime` to the content types you want, and scope
   the plugin to them.
2. Install and enable the plugin (see [Managing plugins](managing.md)).
3. Optionally configure `{"sourceField": "body", "targetField": "readingTime", "wordsPerMinute": 200}`.

A 450-word body yields `readingTime: 3`.

:::tip
Go modules are larger than Rust ones (a few MB), well within the default 20
MiB module limit. TinyGo produces smaller modules if size matters.
:::
