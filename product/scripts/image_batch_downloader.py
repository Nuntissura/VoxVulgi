#!/usr/bin/env python3
"""
Batch image downloader for blog/forum archives.

Features:
- Crawls through pagination ("next page", rel=next, pagination blocks).
- Extracts image URLs from img/srcset/data-* and image links.
- Tries to prefer full-size images over thumbnails.
- Skips likely profile/avatar images by URL/alt/class/id heuristics.
- Deduplicates by SHA-256.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import mimetypes
import re
import sys
import time
from collections import deque
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List, Optional, Set, Tuple
from urllib.parse import parse_qsl, urljoin, urlparse, urlunparse

try:
    import requests
    from bs4 import BeautifulSoup, Tag
except ImportError as exc:  # pragma: no cover
    print(
        "Missing dependency. Install with:\n"
        "  python -m pip install requests beautifulsoup4\n\n"
        f"Details: {exc}",
        file=sys.stderr,
    )
    raise SystemExit(2)


PROFILE_MARKERS = (
    "avatar",
    "profile",
    "userpic",
    "gravatar",
    "author-photo",
    "member-photo",
    "display-picture",
)

THUMB_HINTS = (
    "thumb",
    "thumbnail",
    "_tn",
    "-tn",
    "_sm",
    "-sm",
    "_small",
    "-small",
    "small/",
    "/small",
)

NEXT_TEXT_MARKERS = (
    "next",
    "older",
    "more",
    "weiter",
    "suivant",
    "volgende",
    "nast",
    "›",
    "»",
    ">>",
    ">",
)

IMAGE_ATTRS = ("src", "data-src", "data-original", "data-full", "data-lazy-src")

URL_QUERY_THUMB_KEYS = {
    "w",
    "h",
    "width",
    "height",
    "size",
    "thumb",
    "thumbnail",
    "fit",
    "crop",
    "quality",
}


@dataclass(frozen=True)
class ImageCandidate:
    page_url: str
    urls: Tuple[str, ...]
    skip_profile: bool


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Crawl pages and download full-size images while skipping profile photos."
    )
    parser.add_argument("start_urls", nargs="+", help="One or more start URLs (blog/forum pages).")
    parser.add_argument(
        "--output",
        default="image_archive",
        help="Output directory (default: image_archive).",
    )
    parser.add_argument(
        "--max-pages",
        type=int,
        default=2000,
        help="Maximum HTML pages to crawl (default: 2000).",
    )
    parser.add_argument(
        "--delay-seconds",
        type=float,
        default=0.35,
        help="Delay between page requests in seconds (default: 0.35).",
    )
    parser.add_argument(
        "--timeout-seconds",
        type=float,
        default=25.0,
        help="HTTP request timeout in seconds (default: 25).",
    )
    parser.add_argument(
        "--allow-cross-domain",
        action="store_true",
        help="Allow crawling outside the start URL domains.",
    )
    parser.add_argument(
        "--no-follow-content-links",
        action="store_true",
        help="Only follow pagination links (do not follow post/thread/content links).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Crawl and report what would download, but do not write image files.",
    )
    parser.add_argument(
        "--user-agent",
        default=(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
            "AppleWebKit/537.36 (KHTML, like Gecko) "
            "Chrome/130.0.0.0 Safari/537.36"
        ),
        help="Custom User-Agent header.",
    )
    parser.add_argument(
        "--skip-url-keyword",
        action="append",
        default=[],
        help="Extra lowercase URL keyword to skip (repeatable).",
    )
    return parser.parse_args()


def normalize_url(raw_url: str, base_url: str) -> Optional[str]:
    raw_url = (raw_url or "").strip()
    if not raw_url:
        return None
    if raw_url.startswith(("javascript:", "mailto:", "tel:", "#")):
        return None
    url = urljoin(base_url, raw_url)
    parsed = urlparse(url)
    if parsed.scheme not in ("http", "https"):
        return None
    cleaned = parsed._replace(fragment="")
    return urlunparse(cleaned)


def host_of(url: str) -> str:
    return urlparse(url).netloc.lower()


def looks_like_image_url(url: str) -> bool:
    path = urlparse(url).path.lower()
    return path.endswith(
        (
            ".jpg",
            ".jpeg",
            ".png",
            ".gif",
            ".webp",
            ".bmp",
            ".tif",
            ".tiff",
            ".svg",
            ".avif",
            ".heic",
        )
    )


def sanitize_name(text: str) -> str:
    text = text.strip().lower()
    text = re.sub(r"[^a-z0-9._-]+", "_", text)
    text = text.strip("._")
    return text or "image"


def parse_srcset_best(srcset: str, base_url: str) -> Optional[str]:
    best_url: Optional[str] = None
    best_score = -1
    for chunk in srcset.split(","):
        part = chunk.strip()
        if not part:
            continue
        bits = part.split()
        raw = bits[0]
        candidate = normalize_url(raw, base_url)
        if not candidate:
            continue
        score = 1
        if len(bits) > 1:
            token = bits[1].strip().lower()
            if token.endswith("w"):
                try:
                    score = int(token[:-1])
                except ValueError:
                    score = 1
            elif token.endswith("x"):
                try:
                    score = int(float(token[:-1]) * 1000)
                except ValueError:
                    score = 1
        if score > best_score:
            best_score = score
            best_url = candidate
    return best_url


def strip_thumbnail_query_params(url: str) -> str:
    parsed = urlparse(url)
    if not parsed.query:
        return url
    kept = [(k, v) for k, v in parse_qsl(parsed.query, keep_blank_values=True) if k.lower() not in URL_QUERY_THUMB_KEYS]
    new_query = "&".join(f"{k}={v}" if v != "" else k for k, v in kept)
    return urlunparse(parsed._replace(query=new_query))


def guess_fullsize_variants(url: str) -> List[str]:
    variants = [url]

    cleaned_query = strip_thumbnail_query_params(url)
    if cleaned_query != url:
        variants.append(cleaned_query)

    parsed = urlparse(url)
    path = parsed.path
    path_variants = {
        path.replace("/thumb/", "/"),
        path.replace("/thumbs/", "/"),
        path.replace("/thumbnail/", "/"),
        re.sub(r"(?i)([_-])(thumb|thumbnail|small|sm|tn)\b", "", path),
        re.sub(r"(?i)\b(thumb|thumbnail|small)[_-]", "", path),
    }
    for p in path_variants:
        if not p or p == path:
            continue
        variants.append(urlunparse(parsed._replace(path=p)))

    deduped: List[str] = []
    seen: Set[str] = set()
    for candidate in variants:
        if candidate not in seen:
            seen.add(candidate)
            deduped.append(candidate)
    return deduped


def keyword_match(value: str, keywords: Iterable[str]) -> bool:
    lowered = value.lower()
    return any(k in lowered for k in keywords)


def is_likely_profile_image(tag: Tag, url: str) -> bool:
    url_l = url.lower()
    if keyword_match(url_l, PROFILE_MARKERS):
        return True

    attrs_to_scan = []
    for attr in ("class", "id", "alt", "title"):
        val = tag.get(attr)
        if isinstance(val, list):
            attrs_to_scan.extend(str(v) for v in val)
        elif val:
            attrs_to_scan.append(str(val))

    parent = tag.parent
    if isinstance(parent, Tag):
        for attr in ("class", "id"):
            val = parent.get(attr)
            if isinstance(val, list):
                attrs_to_scan.extend(str(v) for v in val)
            elif val:
                attrs_to_scan.append(str(val))

    return any(keyword_match(text, PROFILE_MARKERS) for text in attrs_to_scan)


def extract_image_candidates(soup: BeautifulSoup, page_url: str) -> List[ImageCandidate]:
    out: List[ImageCandidate] = []

    for img in soup.find_all("img"):
        urls: List[str] = []

        srcset = img.get("srcset")
        if srcset:
            best = parse_srcset_best(str(srcset), page_url)
            if best:
                urls.append(best)

        for attr in IMAGE_ATTRS:
            raw = img.get(attr)
            if raw:
                normalized = normalize_url(str(raw), page_url)
                if normalized:
                    urls.append(normalized)

        parent = img.parent
        if isinstance(parent, Tag) and parent.name == "a":
            href = parent.get("href")
            if href:
                anchor_url = normalize_url(str(href), page_url)
                if anchor_url and looks_like_image_url(anchor_url):
                    urls.insert(0, anchor_url)

        deduped_urls: List[str] = []
        seen_url: Set[str] = set()
        for u in urls:
            for variant in guess_fullsize_variants(u):
                if variant not in seen_url:
                    seen_url.add(variant)
                    deduped_urls.append(variant)

        if deduped_urls:
            skip_profile = is_likely_profile_image(img, deduped_urls[0])
            out.append(ImageCandidate(page_url=page_url, urls=tuple(deduped_urls), skip_profile=skip_profile))

    for a in soup.find_all("a"):
        href = a.get("href")
        if not href:
            continue
        normalized = normalize_url(str(href), page_url)
        if not normalized or not looks_like_image_url(normalized):
            continue
        if keyword_match(normalized, PROFILE_MARKERS):
            continue
        variants = tuple(guess_fullsize_variants(normalized))
        out.append(ImageCandidate(page_url=page_url, urls=variants, skip_profile=False))

    deduped: List[ImageCandidate] = []
    seen_first_url: Set[str] = set()
    for candidate in out:
        key = candidate.urls[0]
        if key in seen_first_url:
            continue
        seen_first_url.add(key)
        deduped.append(candidate)
    return deduped


def is_next_link(tag: Tag, href: str) -> bool:
    rel = " ".join(tag.get("rel", [])).lower() if tag.get("rel") else ""
    if "next" in rel:
        return True

    text = (tag.get_text(" ", strip=True) or "").lower()
    attrs = " ".join(
        [
            str(tag.get("class", "")),
            str(tag.get("id", "")),
            str(tag.get("aria-label", "")),
            str(tag.get("title", "")),
        ]
    ).lower()
    href_l = href.lower()

    if keyword_match(text, NEXT_TEXT_MARKERS):
        return True
    if keyword_match(attrs, ("next", "pagination", "pager", "older", "newer")):
        return True
    if re.search(r"[?&](page|p)=\d+", href_l):
        return True
    return False


def is_probable_content_link(tag: Tag, href: str) -> bool:
    href_l = href.lower()
    attrs = " ".join(
        [
            str(tag.get("class", "")),
            str(tag.get("id", "")),
            str(tag.get("rel", "")),
        ]
    ).lower()
    text = (tag.get_text(" ", strip=True) or "").lower()

    if any(x in href_l for x in ("/post", "/posts/", "/blog/", "/article", "/topic", "/thread", "/forum/")):
        return True
    if any(x in attrs for x in ("post", "entry", "topic", "thread", "article")):
        return True
    if len(text) > 15 and re.search(r"\d{4}", href_l):
        return True
    return False


def discover_links(
    soup: BeautifulSoup,
    page_url: str,
    follow_content_links: bool,
) -> Tuple[Set[str], Set[str]]:
    next_links: Set[str] = set()
    content_links: Set[str] = set()

    for link in soup.find_all("link"):
        href = link.get("href")
        if not href:
            continue
        normalized = normalize_url(str(href), page_url)
        if not normalized:
            continue
        rel = " ".join(link.get("rel", [])).lower() if link.get("rel") else ""
        if "next" in rel:
            next_links.add(normalized)

    for a in soup.find_all("a"):
        href = a.get("href")
        if not href:
            continue
        normalized = normalize_url(str(href), page_url)
        if not normalized:
            continue
        if is_next_link(a, normalized):
            next_links.add(normalized)
        if follow_content_links and is_probable_content_link(a, normalized):
            content_links.add(normalized)

    for node in soup.find_all(attrs={"class": re.compile(r"(pagination|pager|pagenav|nav-links)", re.I)}):
        if not isinstance(node, Tag):
            continue
        for a in node.find_all("a"):
            href = a.get("href")
            if not href:
                continue
            normalized = normalize_url(str(href), page_url)
            if normalized:
                next_links.add(normalized)

    return next_links, content_links


def guess_extension(url: str, content_type: str) -> str:
    path = urlparse(url).path
    suffix = Path(path).suffix.lower()
    if suffix in (".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tif", ".tiff", ".svg", ".avif", ".heic"):
        return suffix
    guessed = mimetypes.guess_extension((content_type or "").split(";")[0].strip().lower() or "")
    return guessed or ".jpg"


def download_image(
    session: requests.Session,
    candidate: ImageCandidate,
    output_dir: Path,
    seen_hashes: Set[str],
    dry_run: bool,
    timeout_seconds: float,
    skip_url_keywords: Iterable[str],
) -> Tuple[str, Optional[Path], Optional[int], Optional[str]]:
    for url in candidate.urls:
        if keyword_match(url, skip_url_keywords):
            return "skipped_custom_keyword", None, None, None
        if candidate.skip_profile:
            return "skipped_profile", None, None, None

        try:
            resp = session.get(url, timeout=timeout_seconds)
        except requests.RequestException:
            continue
        if resp.status_code >= 400:
            continue

        content_type = (resp.headers.get("content-type") or "").lower()
        if "image" not in content_type and not looks_like_image_url(url):
            continue

        data = resp.content
        if not data:
            continue
        if len(data) < 512 and keyword_match(url, THUMB_HINTS):
            continue

        digest = hashlib.sha256(data).hexdigest()
        if digest in seen_hashes:
            return "duplicate", None, len(data), digest

        if dry_run:
            seen_hashes.add(digest)
            return "would_download", None, len(data), digest

        ext = guess_extension(url, content_type)
        stem = sanitize_name(Path(urlparse(url).path).stem or "image")
        filename = f"{stem}_{digest[:12]}{ext}"
        out_path = output_dir / filename
        out_path.write_bytes(data)
        seen_hashes.add(digest)
        return "downloaded", out_path, len(data), digest

    return "failed_all_variants", None, None, None


def is_html_response(resp: requests.Response) -> bool:
    ctype = (resp.headers.get("content-type") or "").lower()
    return "text/html" in ctype or "application/xhtml+xml" in ctype or ctype == ""


def crawl_and_download(args: argparse.Namespace) -> int:
    start_urls = [u for u in (normalize_url(x, x) for x in args.start_urls) if u]
    if not start_urls:
        print("No valid start URLs.", file=sys.stderr)
        return 2

    output_root = Path(args.output).resolve()
    output_root.mkdir(parents=True, exist_ok=True)
    manifest_path = output_root / "manifest.csv"

    allowed_hosts = {host_of(u) for u in start_urls}
    follow_content_links = not args.no_follow_content_links
    skip_keywords = [k.strip().lower() for k in args.skip_url_keyword if k.strip()]

    session = requests.Session()
    session.headers.update({"User-Agent": args.user_agent})

    queue = deque(start_urls)
    visited_pages: Set[str] = set()
    seen_image_urls: Set[str] = set()
    seen_hashes: Set[str] = set()

    total_pages = 0
    downloaded = 0
    would_download = 0
    skipped_profile = 0
    duplicates = 0
    failed = 0

    with manifest_path.open("w", newline="", encoding="utf-8") as fh:
        writer = csv.writer(fh)
        writer.writerow(
            ["page_url", "image_url", "status", "saved_path", "bytes", "sha256", "variant_count"]
        )

        while queue and total_pages < args.max_pages:
            page_url = queue.popleft()
            if page_url in visited_pages:
                continue
            visited_pages.add(page_url)

            if not args.allow_cross_domain and host_of(page_url) not in allowed_hosts:
                continue

            try:
                resp = session.get(page_url, timeout=args.timeout_seconds)
            except requests.RequestException:
                continue

            if resp.status_code >= 400 or not is_html_response(resp):
                continue

            total_pages += 1
            print(f"[page {total_pages}] {page_url}")

            soup = BeautifulSoup(resp.text, "html.parser")
            candidates = extract_image_candidates(soup, page_url)

            host_folder = sanitize_name(host_of(page_url))
            image_out_dir = output_root / host_folder / "images"
            if not args.dry_run:
                image_out_dir.mkdir(parents=True, exist_ok=True)

            for candidate in candidates:
                first_url = candidate.urls[0]
                if first_url in seen_image_urls:
                    continue
                seen_image_urls.add(first_url)

                status, saved_path, byte_count, digest = download_image(
                    session=session,
                    candidate=candidate,
                    output_dir=image_out_dir,
                    seen_hashes=seen_hashes,
                    dry_run=args.dry_run,
                    timeout_seconds=args.timeout_seconds,
                    skip_url_keywords=skip_keywords,
                )

                if status == "downloaded":
                    downloaded += 1
                elif status == "would_download":
                    would_download += 1
                elif status == "skipped_profile":
                    skipped_profile += 1
                elif status == "duplicate":
                    duplicates += 1
                else:
                    failed += 1

                writer.writerow(
                    [
                        page_url,
                        first_url,
                        status,
                        str(saved_path) if saved_path else "",
                        byte_count if byte_count is not None else "",
                        digest if digest else "",
                        len(candidate.urls),
                    ]
                )

            next_links, content_links = discover_links(
                soup, page_url, follow_content_links=follow_content_links
            )
            for link in sorted(next_links | content_links):
                if link in visited_pages:
                    continue
                if not args.allow_cross_domain and host_of(link) not in allowed_hosts:
                    continue
                queue.append(link)

            if args.delay_seconds > 0:
                time.sleep(args.delay_seconds)

    print("\nDone.")
    print(f"Pages crawled: {total_pages}")
    print(f"Images downloaded: {downloaded}")
    print(f"Images (dry-run only): {would_download}")
    print(f"Skipped profile images: {skipped_profile}")
    print(f"Duplicates skipped: {duplicates}")
    print(f"Failed/unsupported: {failed}")
    print(f"Manifest: {manifest_path}")
    return 0


def main() -> int:
    args = parse_args()
    return crawl_and_download(args)


if __name__ == "__main__":
    raise SystemExit(main())
