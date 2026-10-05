"""Owning component negatives; fixtures do not certify package or network compatibility."""
import contextlib
import hashlib
import importlib.util
import pathlib
import tempfile
import types
import unittest
from unittest.mock import patch

path = pathlib.Path(__file__).with_name("hf_owned_progress.py")
spec = importlib.util.spec_from_file_location("owned_http_under_test", path)
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)

class HttpContextTests(unittest.TestCase):
    def make_modules(self, root):
        modules, pins = {}, {}
        for name in helper.MODULES:
            source = root / (name.replace(".", "_") + ".py")
            source.write_text("# explicit component fixture\n", encoding="utf8")
            modules[name] = types.SimpleNamespace(__file__=str(source))
            pins[name] = hashlib.sha256(source.read_bytes()).hexdigest()
        modules["huggingface_hub.constants"].HF_HUB_DISABLE_XET = True
        modules["huggingface_hub.constants"].HF_HUB_ENABLE_HF_TRANSFER = False
        self.counts = []
        self.transfers = []
        outer = self
        class Original:
            def update(self, n): outer.counts.append(n); return "original-result"
            def update_transfer(self, n): outer.transfers.append(n); return "transfer-result"
        @contextlib.contextmanager
        def original(**kwargs): yield kwargs.get("_tqdm_bar") or Original()
        original.__module__ = "huggingface_hub.utils.tqdm"
        original.__name__ = "_get_progress_bar_context"
        modules["huggingface_hub.file_download"]._get_progress_bar_context = original
        return modules, pins, original

    def test_actual_context_adapter_forwards_reconstruction_and_transfer_only_counts_once(self):
        with tempfile.TemporaryDirectory() as directory:
            modules,pins,original = self.make_modules(pathlib.Path(directory))
            events = helper.NonWaitingHandoff()
            with patch.object(helper.importlib,"import_module",side_effect=lambda name:modules[name]), patch.object(helper.importlib.metadata,"version",side_effect=lambda name:{"huggingface_hub":"1.33.0","tqdm":"4.70.1"}[name]):
                with helper.owned_hf_progress(directory,pins,events) as availability:
                    self.assertTrue(availability["available"])
                    with modules["huggingface_hub.file_download"]._get_progress_bar_context(name="huggingface_hub.http_get",total=100,initial=50) as proxy:
                        self.assertEqual(proxy.update(10),"original-result")
                        self.assertEqual(proxy.update_transfer(10),"transfer-result")
                        self.assertEqual(proxy._vv_bytes,60)
                        proxy.update(-50)
                        self.assertEqual(proxy._vv_bytes,10)
                        self.assertEqual(proxy._vv_initial,10)
                self.assertIs(modules["huggingface_hub.file_download"]._get_progress_bar_context,original)
                self.assertEqual(self.counts,[10,-50]);self.assertEqual(self.transfers,[10])

    def test_held_handoff_and_sticky_invalidation_never_claim_valid_old_counter(self):
        events=helper.NonWaitingHandoff(1)
        with events._lock:
            self.assertFalse(events.try_offer({"event":"update","gap":False}))
        self.assertTrue(events.try_offer({"event":"update","gap":False}))
        events.invalidated=True
        state,frame=events.try_take()
        self.assertEqual(state,"frame");self.assertEqual(frame["event"],"invalid");self.assertTrue(frame["gap"])

    def test_unknown_effective_source_does_not_replace_original_context(self):
        with tempfile.TemporaryDirectory() as directory:
            modules,pins,original=self.make_modules(pathlib.Path(directory));pins[helper.MODULES[0]]="0"*64
            with patch.object(helper.importlib,"import_module",side_effect=lambda name:modules[name]), patch.object(helper.importlib.metadata,"version",side_effect=lambda name:{"huggingface_hub":"0.34.4","tqdm":"4.68.3"}[name]):
                with helper.owned_hf_progress(directory,pins,helper.NonWaitingHandoff()) as availability:self.assertFalse(availability["available"])
            self.assertIs(modules["huggingface_hub.file_download"]._get_progress_bar_context,original)



# Execute only the proposal definitions, never a pip module or download in component tests.
exec(compile(pathlib.Path(__file__).with_name("pip_transfer_wrapper.py").read_text(encoding="utf8"), "<pip-helper-under-test>", "exec"), helper.__dict__)
class PipGeneratorTests(unittest.TestCase):
    def test_delegates_same_chunks_original_raw_output_and_closes_original_on_cancel(self):
        chunks=[b"abc",b"def"]
        written=[];closed=[]
        def original(iterable, *, size, initial_progress=None):
            try:
                current=initial_progress or 0
                written.append((current,size))
                for chunk in iterable:
                    current+=len(chunk);written.append((current,size));yield chunk
            finally:
                closed.append(True)
        module=types.SimpleNamespace(_raw_progress_bar=original)
        events=helper.NonWaitingHandoff()
        with helper.owned_pip_progress(module,events):
            iterator=module._raw_progress_bar(chunks,size=16,initial_progress=10)
            self.assertIs(next(iterator),chunks[0])
            self.assertEqual(written,[(10,16),(13,16)])
            iterator.close()
        self.assertEqual(closed,[True]);self.assertIs(module._raw_progress_bar,original)
        frames=[]
        while True:
            state,frame=events.try_take()
            if state!="frame":break
            frames.append(frame)
        self.assertEqual([frame["event"] for frame in frames],["begin","update","bar_closed"])
        self.assertEqual(frames[1]["received_bytes"],13)
        self.assertEqual(frames[1]["counter_source"],"original_pip_raw_payload_sum")
    def test_preserves_original_iteration_failure_and_unknown_total(self):
        def original(iterable, *, size, initial_progress=None):
            yield next(iter(iterable))
            raise RuntimeError("original failure")
        module=types.SimpleNamespace(_raw_progress_bar=original)
        events=helper.NonWaitingHandoff()
        with self.assertRaisesRegex(RuntimeError,"original failure"):
            with helper.owned_pip_progress(module,events):
                iterator=module._raw_progress_bar([b"abc"],size=None)
                self.assertEqual(next(iterator),b"abc")
                next(iterator)
        self.assertIs(module._raw_progress_bar,original)
        state,begin=events.try_take();self.assertEqual(state,"frame");self.assertIsNone(begin["total_bytes"])



class SourceClockAndRetryTests(unittest.TestCase):
    make_modules = HttpContextTests.make_modules
    def test_boot_age_is_captured_before_monotonic_frame_preparation(self):
        order=[];events=helper.NonWaitingHandoff()
        bar=types.SimpleNamespace(_vv_id=1,_vv_bytes=10,_vv_initial=0,_vv_total=100,_vv_lost=False)
        with patch.object(helper,"_vv_boot_tick_ms",side_effect=lambda:order.append("boot") or 1000), patch.object(helper.time,"monotonic_ns",side_effect=lambda:order.append("monotonic") or 42):
            helper._publish_transfer(events,bar,"update",10)
        self.assertEqual(order,["boot","monotonic"])
        state,frame=events.try_take();self.assertEqual(state,"frame");self.assertEqual(frame["observed_boot_tick_ms"],1000)
    def test_genuine_recursive_shared_bar_resets_only_telemetry_generation(self):
        with tempfile.TemporaryDirectory() as directory:
            modules,pins,original=self.make_modules(pathlib.Path(directory));events=helper.NonWaitingHandoff()
            with patch.object(helper.importlib,"import_module",side_effect=lambda name:modules[name]), patch.object(helper.importlib.metadata,"version",side_effect=lambda name:{"huggingface_hub":"1.33.0","tqdm":"4.70.1"}[name]):
                with helper.owned_hf_progress(directory,pins,events):
                    context=modules["huggingface_hub.file_download"]._get_progress_bar_context
                    with context(name="huggingface_hub.http_get",total=100,initial=50) as proxy:
                        proxy.update(10);old_id=proxy._vv_id;underlying=proxy._progress
                        with context(_tqdm_bar=proxy) as retry:
                            self.assertIs(retry,proxy);self.assertIs(retry._progress,underlying)
                            self.assertNotEqual(retry._vv_id,old_id);self.assertEqual(retry._vv_initial,60)
                            retry.update(5)
                        self.assertEqual(proxy._vv_bytes,65)
                self.assertIs(modules["huggingface_hub.file_download"]._get_progress_bar_context,original)
                self.assertEqual(self.counts,[10,5])

if __name__ == "__main__": unittest.main()
