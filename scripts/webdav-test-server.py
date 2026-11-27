"""Minimal read-only WebDAV server for DiffEverything smoke tests."""
import argparse
import html
import pathlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote, urlsplit


class WebDavHandler(BaseHTTPRequestHandler):
    root: pathlib.Path

    def translate(self):
        path = unquote(urlsplit(self.path).path)
        candidate = (self.root / path.strip("/")).resolve()
        if candidate != self.root and self.root not in candidate.parents:
            return None
        return candidate

    def do_GET(self):
        target = self.translate()
        if target is None or not target.is_file():
            self.send_error(404)
            return
        data = target.read_bytes()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_PROPFIND(self):
        target = self.translate()
        if target is None or not target.is_dir():
            self.send_error(404)
            return
        rel = unquote(urlsplit(self.path).path).strip("/")
        rows = []

        def add(name: str, is_dir: bool) -> None:
            href = "/" + "/".join(part for part in (rel, name) if part) + ("/" if is_dir else "")
            collection = "<d:collection/>" if is_dir else ""
            rows.append(
                "<d:response><d:href>"
                + html.escape(href)
                + "</d:href><d:propstat><d:prop><d:resourcetype>"
                + collection
                + "</d:resourcetype></d:prop></d:propstat></d:response>"
            )

        add("", True)
        for entry in sorted(target.iterdir()):
            add(entry.name, entry.is_dir())
        body = (
            '<?xml version="1.0" encoding="utf-8"?>'
            '<d:multistatus xmlns:d="DAV:">' + "".join(rows) + "</d:multistatus>"
        ).encode()
        self.send_response(207)
        self.send_header("Content-Type", "application/xml; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True)
    parser.add_argument("--port", type=int, default=8080)
    args = parser.parse_args()

    WebDavHandler.root = pathlib.Path(args.root).resolve()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), WebDavHandler)
    print(f"READY {args.port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
