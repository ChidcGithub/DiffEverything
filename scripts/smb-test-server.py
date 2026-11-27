"""Local SMB2 server for DiffEverything integration tests (impacket)."""
import argparse
import pathlib
import sys

from impacket import smbserver
from impacket.ntlm import compute_lmhash, compute_nthash


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True)
    parser.add_argument("--port", type=int, default=4455)
    parser.add_argument("--user", default="test")
    parser.add_argument("--password", default="test")
    args = parser.parse_args()

    root = pathlib.Path(args.root).resolve()
    server = smbserver.SimpleSMBServer(listenAddress="127.0.0.1", listenPort=args.port)
    server.addShare("data", str(root), readOnly="yes")
    server.setSMB2Support(True)
    server.addCredential(
        args.user,
        0,
        compute_lmhash(args.password),
        compute_nthash(args.password),
    )
    print(f"READY {args.port}", flush=True)
    try:
        server.start()
    except KeyboardInterrupt:
        sys.exit(0)


if __name__ == "__main__":
    main()
