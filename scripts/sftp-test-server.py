"""Minimal local SFTP server for DiffEverything integration tests."""
import argparse
import asyncio
import pathlib
import sys

import asyncssh


class Server(asyncssh.SSHServer):
    def __init__(self, root: pathlib.Path, username: str, password: str):
        self.root = root
        self.username = username
        self.password = password

    def begin_auth(self, username):
        return True

    def password_auth_supported(self):
        return True

    def validate_password(self, username, password):
        return username == self.username and password == self.password


async def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True)
    parser.add_argument("--port", type=int, default=2222)
    parser.add_argument("--user", default="test")
    parser.add_argument("--password", default="test")
    parser.add_argument(
        "--host-key-out",
        help="write the OpenSSH public host key line to this file",
    )
    args = parser.parse_args()

    root = pathlib.Path(args.root).resolve()
    host_key = asyncssh.generate_private_key("ssh-rsa", key_size=2048)
    if args.host_key_out:
        public_key = host_key.export_public_key().decode().strip()
        with open(args.host_key_out, "w", encoding="ascii", newline="\n") as handle:
            handle.write(public_key + "\n")

    def factory():
        return Server(root, args.user, args.password)

    await asyncssh.create_server(
        factory,
        "127.0.0.1",
        args.port,
        server_host_keys=[host_key],
        encryption_algs=["aes256-ctr", "aes128-ctr"],
        kex_algs=["diffie-hellman-group14-sha256", "diffie-hellman-group14-sha1"],
        sftp_factory=lambda channel: asyncssh.SFTPServer(channel, chroot=str(root)),
    )
    print(f"READY {args.port}", flush=True)
    await asyncio.Future()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.exit(0)
