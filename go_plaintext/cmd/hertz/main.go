package main

import (
	"context"
	"flag"
	"log"

	"github.com/cloudwego/hertz/pkg/app"
	"github.com/cloudwego/hertz/pkg/app/server"
)

func main() {
	addr := flag.String("addr", "127.0.0.1:28082", "listen address")
	flag.Parse()

	h := server.New(server.WithHostPorts(*addr))

	h.GET("/health", func(_ context.Context, ctx *app.RequestContext) {
		ctx.Status(200)
	})

	h.GET("/plaintext", func(_ context.Context, ctx *app.RequestContext) {
		ctx.Response.Header.SetContentTypeBytes([]byte("text/plain; charset=utf-8"))
		ctx.String(200, "OK")
	})

	log.Printf("listening on %s", *addr)
	h.Spin()
}
