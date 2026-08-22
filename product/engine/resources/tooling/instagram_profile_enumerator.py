import argparse
import json
import sys
from itertools import islice

import instaloader


def _cookie_header(path):
    if not path:
        return {}
    text = open(path, "r", encoding="utf-8").read().strip()
    cookies = {}
    for part in text.split(";"):
        if "=" not in part:
            continue
        name, value = part.split("=", 1)
        name = name.strip()
        if name:
            cookies[name] = value.strip()
    return cookies


def _post_assets(post):
    if post.typename == "GraphSidecar":
        assets = [
            {
                "asset_index": index,
                "media_kind": "video" if node.is_video else "image",
                "download_url": node.video_url if node.is_video else node.display_url,
            }
            for index, node in enumerate(post.get_sidecar_nodes())
        ]
    else:
        assets = [
            {
                "asset_index": 0,
                "media_kind": "video" if post.is_video else "image",
                "download_url": post.video_url if post.is_video else post.url,
            }
        ]
    assets = [asset for asset in assets if asset["download_url"]]
    if not assets:
        raise RuntimeError(f"Instagram post {post.shortcode} returned no downloadable assets")
    return assets


def _post(post, kind):
    caption = (post.caption or "").strip()
    source_kind = "reel" if kind == "reel" else "p"
    return {
        "media_id": str(post.mediaid),
        "shortcode": post.shortcode,
        "kind": kind,
        "source_url": f"https://www.instagram.com/{source_kind}/{post.shortcode}/",
        "title": caption.splitlines()[0][:500] if caption else None,
        "description": caption or None,
        "creator_id": str(post.owner_id),
        "creator_name": post.owner_username,
        "published_at_ms": int(post.date_utc.timestamp() * 1000),
        "thumbnail_url": post.url,
        "duration_seconds": getattr(post, "video_duration", None),
        "assets": _post_assets(post),
    }


def _story(item, profile):
    caption = (item.caption or "").strip()
    return {
        "media_id": str(item.mediaid),
        "shortcode": item.shortcode,
        "kind": "story",
        "source_url": f"https://www.instagram.com/stories/{profile.username}/{item.mediaid}/",
        "title": caption.splitlines()[0][:500] if caption else None,
        "description": caption or None,
        "creator_id": str(item.owner_id),
        "creator_name": profile.username,
        "published_at_ms": int(item.date_utc.timestamp() * 1000),
        "thumbnail_url": item.url,
        "duration_seconds": getattr(item, "video_duration", None),
        "assets": [
            {
                "asset_index": 0,
                "media_kind": "video" if item.is_video else "image",
                "download_url": item.video_url if item.is_video else item.url,
            }
        ],
    }


def main():
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    target = parser.add_mutually_exclusive_group(required=True)
    target.add_argument("--profile")
    target.add_argument("--post-shortcode")
    parser.add_argument("--post-kind", choices=["post", "reel"], default="post")
    parser.add_argument("--max-items", type=int, default=1)
    parser.add_argument("--include-posts", action="store_true")
    parser.add_argument("--include-reels", action="store_true")
    parser.add_argument("--include-stories", action="store_true")
    parser.add_argument("--session-file")
    parser.add_argument("--session-user")
    parser.add_argument("--cookie-header-file")
    args = parser.parse_args()

    loader = instaloader.Instaloader(
        quiet=True,
        sleep=True,
        download_pictures=False,
        download_videos=False,
        download_video_thumbnails=False,
        download_geotags=False,
        download_comments=False,
        save_metadata=False,
        compress_json=False,
        max_connection_attempts=3,
        request_timeout=30.0,
    )
    if args.session_file:
        if not args.session_user:
            raise RuntimeError("session user is required with session file")
        loader.load_session_from_file(args.session_user, args.session_file)
    else:
        cookies = _cookie_header(args.cookie_header_file)
        if cookies:
            loader.context.update_cookies(cookies)
            login_user = loader.test_login()
            if login_user:
                loader.context.username = login_user

    if args.post_shortcode:
        post = instaloader.Post.from_shortcode(loader.context, args.post_shortcode)
        item = _post(post, args.post_kind)
        print(
            json.dumps(
                {
                    "provider": "instaloader",
                    "provider_version": instaloader.__version__,
                    "profile": {
                        "id": str(post.owner_id),
                        "username": post.owner_username,
                        "full_name": None,
                        "source_url": f"https://www.instagram.com/{post.owner_username}/",
                    },
                    "items": [item],
                    "capability_errors": {},
                },
                ensure_ascii=False,
            )
        )
        return

    profile = instaloader.Profile.from_username(loader.context, args.profile)
    limit = max(1, min(args.max_items, 500))
    candidates = []
    capability_errors = {}
    if args.include_posts:
        try:
            candidates.extend(_post(post, "post") for post in islice(profile.get_posts(), limit))
        except Exception as error:
            capability_errors["posts"] = f"{type(error).__name__}: {error}"
    if args.include_reels:
        try:
            candidates.extend(_post(post, "reel") for post in islice(profile.get_reels(), limit))
        except Exception as error:
            capability_errors["reels"] = f"{type(error).__name__}: {error}"
    if args.include_stories:
        try:
            for story in loader.get_stories(userids=[profile.userid]):
                candidates.extend(_story(item, profile) for item in islice(story.get_items(), limit))
        except Exception as error:
            capability_errors["stories"] = f"{type(error).__name__}: {error}"

    by_media_id = {}
    for item in candidates:
        current = by_media_id.get(item["media_id"])
        if current is None or item["kind"] == "reel":
            by_media_id[item["media_id"]] = item
    items = sorted(
        by_media_id.values(), key=lambda item: item.get("published_at_ms") or 0, reverse=True
    )[:limit]
    print(
        json.dumps(
            {
                "provider": "instaloader",
                "provider_version": instaloader.__version__,
                "profile": {
                    "id": str(profile.userid),
                    "username": profile.username,
                    "full_name": profile.full_name,
                    "source_url": f"https://www.instagram.com/{profile.username}/",
                },
                "items": items,
                "capability_errors": capability_errors,
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(json.dumps({"error": str(error), "type": type(error).__name__}), file=sys.stderr)
        raise
