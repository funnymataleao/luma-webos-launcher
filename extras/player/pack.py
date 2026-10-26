#!/usr/bin/env python3
# Packs app/ into a webOS IPK (ar: debian-binary, control.tar.gz, data.tar.gz).
import io, json, os, sys, tarfile, time

here = os.path.dirname(os.path.abspath(__file__))
app = os.path.join(here, "app")
info = json.load(open(os.path.join(app, "appinfo.json")))
aid, ver = info["id"], info["version"]

def tgz(entries):
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz", format=tarfile.GNU_FORMAT) as t:
        for name, data, mode in entries:
            ti = tarfile.TarInfo(name)
            ti.mtime = int(time.time())
            if data is None:
                ti.type = tarfile.DIRTYPE; ti.mode = 0o755
                t.addfile(ti)
            else:
                ti.size = len(data); ti.mode = mode
                t.addfile(ti, io.BytesIO(data))
    return buf.getvalue()

base = f"./usr/palm/applications/{aid}"
entries = [("./", None, 0), ("./usr", None, 0), ("./usr/palm", None, 0),
           ("./usr/palm/applications", None, 0), (base, None, 0)]
size = 0
for root, dirs, files in os.walk(app):
    rel = os.path.relpath(root, app)
    if rel != ".":
        entries.append((f"{base}/{rel}", None, 0))
    for f in sorted(files):
        data = open(os.path.join(root, f), "rb").read()
        size += len(data)
        entries.append((f"{base}/{rel}/{f}".replace("/./", "/"), data, 0o644))
data_tgz = tgz(entries)
control = (f"Package: {aid}\nVersion: {ver}\nSection: misc\nPriority: optional\n"
           f"Architecture: all\nInstalled-Size: {size}\nMaintainer: Luma\n"
           f"Description: {info['title']}\nwebOS-Package-Format-Version: 2\n"
           f"webOS-Packager-Version: x.y.x\n").encode()
control_tgz = tgz([("./", None, 0), ("./control", control, 0o644)])

def member(name, data):
    h = f"{name + '/':<16}{int(time.time()):<12}{0:<6}{0:<6}{'100644':<8}{len(data):<10}`\n".encode()
    return h + data + (b"\n" if len(data) % 2 else b"")

out = os.path.join(here, f"{aid}_{ver}_all.ipk")
with open(out, "wb") as f:
    f.write(b"!<arch>\n")
    f.write(member("debian-binary", b"2.0\n"))
    f.write(member("control.tar.gz", control_tgz))
    f.write(member("data.tar.gz", data_tgz))
print(out)
