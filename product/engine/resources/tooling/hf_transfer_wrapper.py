# Observer only. Original child callbacks never perform IO or wait for this daemon.
def _vv_start_observer(events, nonce):
    import json
    import os
    stop = threading.Event()
    def drain():
        while not stop.is_set():
            state, frame = events.try_take()
            if state != "frame":
                stop.wait(0.025)
                continue
            try:
                if events.invalidated:
                    frame["event"] = "invalid"
                    frame["gap"] = True
                data = ("@@VV_TRANSFER " + nonce + " " + json.dumps(frame, separators=(",", ":")) + "\n").encode("ascii")
                if len(data) > 4096:
                    continue
                view = memoryview(data)
                while view and not stop.is_set():
                    count = os.write(2, view)
                    if count <= 0:
                        return
                    view = view[count:]
            except Exception:
                return
    try:
        threading.Thread(target=drain, name="vv-owned-transfer", daemon=True).start()
    except Exception:
        events.invalidated = True
        stop.set()  # Observer admission failure cannot fail the original installer.
    return stop

def _vv_run_original(original_code, private_site_packages, expected_module_pins):
    import os
    events = NonWaitingHandoff()
    stop = _vv_start_observer(events, os.environ.get("VOXVULGI_TRANSFER_NONCE", ""))
    try:
        with owned_hf_progress(private_site_packages, expected_module_pins, events):
            namespace = {"__name__": "__main__", "__builtins__": __builtins__}
            exec(compile(original_code, "<string>", "exec"), namespace, namespace)
    finally:
        stop.set()  # No original callback/terminal IO or join.
