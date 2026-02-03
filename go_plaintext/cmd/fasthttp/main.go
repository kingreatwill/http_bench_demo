package main

import (
	"flag"
	"log"

	"github.com/valyala/fasthttp"
)

var plaintextBody = []byte("OK")

func main() {
	addr := flag.String("addr", "127.0.0.1:28081", "listen address")
	flag.Parse()

	handler := func(ctx *fasthttp.RequestCtx) {
		switch string(ctx.Path()) {
		case "/health":
			ctx.SetStatusCode(fasthttp.StatusOK)
		case "/plaintext":
			ctx.SetStatusCode(fasthttp.StatusOK)
			ctx.Response.Header.SetBytesKV([]byte("Content-Type"), []byte("text/plain; charset=utf-8"))
			ctx.SetBody(plaintextBody)
		default:
			ctx.SetStatusCode(fasthttp.StatusNotFound)
		}
	}

	srv := &fasthttp.Server{
		Handler:                       handler,
		Name:                          "fasthttp",
		NoDefaultServerHeader:         true,
		NoDefaultDate:                 true,
		NoDefaultContentType:          true,
		DisableHeaderNamesNormalizing: true,
	}

	log.Printf("listening on %s", *addr)
	log.Fatal(srv.ListenAndServe(*addr))
}
