@contextlib.contextmanager
def owned_pip_progress(module, events):
    original = module._raw_progress_bar
    ids = itertools.count(1)
    active = True
    def observed(iterable, *, size, initial_progress=None):
        inner = original(iterable, size=size, initial_progress=initial_progress)
        initial = initial_progress or 0
        supported = _integer(initial) and (size is None or (_integer(size) and (size == 0 or initial <= size)))
        class Bar: pass
        bar = Bar()
        bar._vv_id = next(ids)
        bar._vv_initial = bar._vv_bytes = initial
        bar._vv_total = size if size else None
        bar._vv_lost = False
        bar._vv_counter_source = "original_pip_raw_payload_sum"
        if supported and active:
            _publish_transfer(events, bar, "begin")
        try:
            for chunk in inner:  # Original generator already counted/wrote; no independent source iteration.
                if supported and active:
                    try:
                        if not isinstance(chunk, bytes) or bar._vv_bytes + len(chunk) > MAX_BYTES or (bar._vv_total is not None and bar._vv_bytes + len(chunk) > bar._vv_total):
                            _publish_transfer(events, bar, "invalid")
                            supported = False
                        else:
                            bar._vv_bytes += len(chunk)
                            _publish_transfer(events, bar, "update", len(chunk))
                    except Exception:
                        events.invalidated = True
                        supported = False
                yield chunk  # Same original object/results/errors, exactly once.
        finally:
            inner.close()  # Exact pinned original has no raising cleanup; GeneratorExit reaches it.
            if supported and active:
                _publish_transfer(events, bar, "bar_closed")
    module._raw_progress_bar = observed
    try:
        yield
    finally:
        active = False
        if module._raw_progress_bar is observed:
            module._raw_progress_bar = original

def _vv_run_original_pip(original_args, expected_files, private_site_packages):
    import os
    import runpy
    import sys
    allowed = False
    try:
        root = pathlib.Path(private_site_packages).resolve(strict=True)
        modules = {"pip/__init__.py": "pip", "pip/_internal/cli/progress_bars.py": "pip._internal.cli.progress_bars", "pip/_internal/cli/cmdoptions.py": "pip._internal.cli.cmdoptions", "pip/_internal/cli/spinners.py": "pip._internal.cli.spinners"}
        loaded = {}
        for relative, name in modules.items():
            module = importlib.import_module(name)
            path = pathlib.Path(module.__file__).resolve(strict=True)
            if not path.is_relative_to(root) or path != (root / relative).resolve(strict=True) or path.stat().st_size > 2097152 or hashlib.sha256(path.read_bytes()).hexdigest() != expected_files[relative]:
                raise ValueError("effective pip source mismatch")
            loaded[name] = module
        progress = loaded["pip._internal.cli.progress_bars"]
        if progress._raw_progress_bar.__module__ != progress.__name__ or progress._raw_progress_bar.__name__ != "_raw_progress_bar":
            raise ValueError("original raw generator reference not proven")
        allowed = True
    except Exception:
        pass  # Unsupported/shadowed original source retains original arguments/function.
    sys.argv = ["pip"] + list(original_args) + (["--progress-bar=raw"] if allowed else [])
    if not allowed:
        return runpy.run_module("pip", run_name="__main__", alter_sys=True)
    events = NonWaitingHandoff()
    stop = _vv_start_observer(events, os.environ.get("VOXVULGI_TRANSFER_NONCE", ""))
    try:
        with owned_pip_progress(progress, events):
            return runpy.run_module("pip", run_name="__main__", alter_sys=True)
    finally:
        stop.set()
