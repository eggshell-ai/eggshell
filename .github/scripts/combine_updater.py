import os
import json
import urllib.request
import urllib.error

def main():
    token = os.environ.get("GITHUB_TOKEN")
    repo = os.environ.get("GITHUB_REPOSITORY")
    tag = os.environ.get("TAG_NAME")

    if not token or not repo or not tag:
        raise ValueError("Missing environment variables GITHUB_TOKEN, GITHUB_REPOSITORY, or TAG_NAME.")

    print(f"Combining updater metadata for {repo} tag {tag}...")

    # 1. Fetch release details from GitHub API
    api_url = f"https://api.github.com/repos/{repo}/releases/tags/{tag}"
    req = urllib.request.Request(
        api_url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "User-Agent": "Tauri-Updater-Builder",
        },
    )

    with urllib.request.urlopen(req) as resp:
        release_data = json.loads(resp.read().decode("utf-8"))

    # Extract clean version without leading 'v'
    version = tag.lstrip("v")
    assets = release_data.get("assets", [])

    # Index .sig assets by their file name
    sig_assets = {a["name"]: a for a in assets if a["name"].endswith(".sig")}
    platforms_data = {}

    print(f"Found {len(assets)} total assets in release.")

    for asset in assets:
        name = asset["name"]
        download_url = asset["browser_download_url"]

        if name.endswith(".sig") or name == "latest.json":
            continue

        sig_name = f"{name}.sig"
        if sig_name not in sig_assets:
            continue

        # Download the signature content
        sig_url = sig_assets[sig_name]["browser_download_url"]
        req_sig = urllib.request.Request(
            sig_url,
            headers={
                "Authorization": f"Bearer {token}",
                "User-Agent": "Tauri-Updater-Builder",
            },
        )
        try:
            with urllib.request.urlopen(req_sig) as sig_resp:
                signature_content = sig_resp.read().decode("utf-8").strip()
        except Exception as e:
            print(f"Warning: Could not download signature for {name}: {e}")
            continue

        # Map assets to Tauri updater platform targets
        # Windows: NSIS exe / MSI
        if name.endswith(".exe") or name.endswith("-setup.exe"):
            platforms_data["windows-x86_64"] = {"signature": signature_content, "url": download_url}
            platforms_data["windows-x86_64-nsis"] = {"signature": signature_content, "url": download_url}
            print(f"Added windows-x86_64 (nsis) target mapping from {name}")
        elif name.endswith(".msi"):
            platforms_data["windows-x86_64-msi"] = {"signature": signature_content, "url": download_url}
            if "windows-x86_64" not in platforms_data:
                platforms_data["windows-x86_64"] = {"signature": signature_content, "url": download_url}
            print(f"Added windows-x86_64 (msi) target mapping from {name}")
        # macOS: tar.gz app / dmg
        elif name.endswith("aarch64.app.tar.gz"):
            platforms_data["darwin-aarch64"] = {"signature": signature_content, "url": download_url}
            platforms_data["darwin-aarch64-app"] = {"signature": signature_content, "url": download_url}
            print(f"Added darwin-aarch64 target mapping from {name}")
        elif name.endswith("x86_64.app.tar.gz") or name.endswith("x64.app.tar.gz"):
            platforms_data["darwin-x86_64"] = {"signature": signature_content, "url": download_url}
            platforms_data["darwin-x86_64-app"] = {"signature": signature_content, "url": download_url}
            print(f"Added darwin-x86_64 target mapping from {name}")
        # Linux: AppImage / deb / rpm
        elif name.endswith("amd64.AppImage") or name.endswith("x86_64.AppImage"):
            platforms_data["linux-x86_64"] = {"signature": signature_content, "url": download_url}
            platforms_data["linux-x86_64-appimage"] = {"signature": signature_content, "url": download_url}
            print(f"Added linux-x86_64 (AppImage) target mapping from {name}")
        elif name.endswith("amd64.deb"):
            platforms_data["linux-x86_64-deb"] = {"signature": signature_content, "url": download_url}
            print(f"Added linux-x86_64 (deb) target mapping from {name}")
        elif name.endswith("x86_64.rpm"):
            platforms_data["linux-x86_64-rpm"] = {"signature": signature_content, "url": download_url}
            print(f"Added linux-x86_64 (rpm) target mapping from {name}")

    latest_json = {
        "version": version,
        "notes": release_data.get("body") or "Release updates and improvements.",
        "pub_date": release_data.get("published_at") or release_data.get("created_at"),
        "platforms": platforms_data,
    }

    output_filename = "latest.json"
    with open(output_filename, "w", encoding="utf-8") as f:
        json.dump(latest_json, f, indent=2)
    print(f"Successfully generated {output_filename} with platforms: {list(platforms_data.keys())}")

    # Delete existing latest.json from release if present
    for asset in assets:
        if asset["name"] == output_filename:
            del_req = urllib.request.Request(
                asset["url"],
                method="DELETE",
                headers={
                    "Authorization": f"Bearer {token}",
                    "Accept": "application/vnd.github+json",
                    "User-Agent": "Tauri-Updater-Builder",
                },
            )
            try:
                urllib.request.urlopen(del_req)
                print("Deleted existing latest.json from release.")
            except Exception as e:
                print(f"Could not delete old latest.json: {e}")

    # Upload fresh latest.json to release
    upload_url_template = release_data.get("upload_url", "")
    upload_url = upload_url_template.split("{")[0] + f"?name={output_filename}"

    with open(output_filename, "rb") as f:
        file_data = f.read()

    upload_req = urllib.request.Request(
        upload_url,
        data=file_data,
        method="POST",
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/octet-stream",
            "Content-Length": str(len(file_data)),
            "User-Agent": "Tauri-Updater-Builder",
        },
    )

    try:
        with urllib.request.urlopen(upload_req) as upload_resp:
            print(f"Successfully uploaded latest.json to release! Status: {upload_resp.status}")
    except urllib.error.HTTPError as e:
        print(f"Failed to upload latest.json: {e.code} - {e.read().decode('utf-8', errors='ignore')}")
        raise

if __name__ == "__main__":
    main()
