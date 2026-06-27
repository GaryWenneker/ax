package main

import "net/http"

func listUsers(w http.ResponseWriter, r *http.Request) {
    w.Write([]byte("ok"))
}

func main() {
    mux := http.NewServeMux()
    mux.HandleFunc("/users", listUsers)
}