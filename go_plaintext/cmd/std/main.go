package main

import (
	"flag"
	"log"
	"net/http"
)

var plaintextBody = []byte("OK")

func main() {
	addr := flag.String("addr", "127.0.0.1:28080", "listen address")
	flag.Parse()

	mux := http.NewServeMux()
	mux.HandleFunc("/health", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Length", "0")
		w.WriteHeader(http.StatusOK)
	})
	mux.HandleFunc("/plaintext", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		w.Header().Set("Content-Length", "2")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write(plaintextBody)
	})

	srv := &http.Server{
		Addr:    *addr,
		Handler: mux,
	}

	log.Printf("listening on %s", *addr)
	log.Fatal(srv.ListenAndServe())
}
