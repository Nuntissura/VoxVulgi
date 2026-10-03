"""Owning publication tests; requires the installed yt-dlp module, no provider access."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SOURCE = Path(__file__).parents[1] / 'resources' / 'tooling' / 'voxvulgi_archive_policy.py'
spec = importlib.util.spec_from_file_location('archive_policy', SOURCE)
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


class ArchiveAudioMetadataTests(unittest.TestCase):
    def publish(self, info, observed):
        with tempfile.TemporaryDirectory() as folder:
            filename = Path(folder) / 'owned.mkv'
            filename.write_bytes(b'original')
            info = dict(info, filepath=str(filename), ext='mkv')
            pp = policy.VoxVulgiArchiveMetadataPP()
            pp.get_metadata_object = lambda path: {'streams': observed}
            calls = []
            def remux(source, output, options):
                calls.append(list(options))
                Path(output).write_bytes(b'remuxed')
            pp.run_ffmpeg = remux
            _, info = pp.run(info)
            return calls, info, filename.read_bytes()

    def test_conflicting_container_language_corrected_from_exact_selection_without_subtitles(self):
        calls, info, output = self.publish({'requested_formats': [
            {'acodec': 'none', 'format_id': 'video'}, {'acodec': 'opus', 'language': 'ko', 'format_id': 'audio'}]},
            [{'codec_type': 'audio', 'tags': {'language': 'eng'}}])
        self.assertIn('-metadata:s:a:0', calls[0])
        self.assertIn('language=kor', calls[0])
        self.assertEqual(output, b'remuxed')
        self.assertEqual(info['voxvulgi_archive_audio_metadata']['tracks'][0]['format_id'], 'audio')

    def test_nested_selection_order_and_specific_audio_title(self):
        calls, _, _ = self.publish({'requested_downloads': [{'requested_formats': [
            {'acodec': 'opus', 'language': 'en', 'audio_track': {'display_name': 'English dub'}},
            {'acodec': 'opus', 'language': 'ko'}]}]}, [{'codec_type': 'audio'}, {'codec_type': 'audio'}])
        args = calls[0]
        self.assertEqual(args[4:8], ['-metadata:s:a:0', 'language=eng', '-metadata:s:a:0', 'title=English dub'])
        self.assertEqual(args[8:10], ['-metadata:s:a:1', 'language=kor'])

    def test_unknown_language_preserves_container_tag_and_ignores_top_level_hint(self):
        calls, info, output = self.publish({'language': 'ko', 'title': 'Video title',
            'requested_formats': [{'acodec': 'opus'}]}, [{'codec_type': 'audio', 'tags': {'language': 'eng'}}])
        self.assertEqual(calls, [])
        self.assertEqual(output, b'original')
        self.assertIsNone(info['voxvulgi_archive_audio_metadata']['tracks'][0]['language'])

    def test_unknown_track_does_not_shift_known_track_index(self):
        calls, _, _ = self.publish({'requested_formats': [{'acodec': 'opus'}, {'acodec': 'opus', 'language': 'ko'}]},
            [{'codec_type': 'audio'}, {'codec_type': 'audio'}])
        self.assertEqual(calls[0][4:], ['-metadata:s:a:1', 'language=kor'])

    def test_progressive_multi_audio_is_rejected_without_remux(self):
        with self.assertRaisesRegex(policy.PostProcessingError, 'cardinality'):
            self.publish({'acodec': 'aac', 'language': 'ko'}, [{'codec_type': 'audio'}, {'codec_type': 'audio'}])

    def test_missing_selected_codec_is_rejected(self):
        with self.assertRaisesRegex(policy.PostProcessingError, 'explicit selected codec'):
            self.publish({'language': 'ko'}, [{'codec_type': 'audio'}])

    def test_multiple_downloads_are_ambiguous(self):
        with self.assertRaisesRegex(policy.PostProcessingError, 'ambiguous'):
            self.publish({'requested_downloads': [{'acodec': 'opus'}, {'acodec': 'opus'}]},
                         [{'codec_type': 'audio'}, {'codec_type': 'audio'}])

    def test_selected_track_missing_is_rejected(self):
        with self.assertRaisesRegex(policy.PostProcessingError, 'cardinality'):
            self.publish({'requested_formats': [{'acodec': 'opus', 'language': 'ko'}]}, [])

    def test_subtitle_metadata_and_all_stream_copy_preserved(self):
        calls, _, _ = self.publish({'acodec': 'opus', 'language': 'ko',
            'requested_subtitles': {'en': {'name': 'English captions'}}},
            [{'codec_type': 'video'}, {'codec_type': 'audio'}, {'codec_type': 'subtitle'}])
        self.assertEqual(calls[0][:4], ['-map', '0', '-c', 'copy'])
        self.assertIn('language=kor', calls[0])
        self.assertIn('-metadata:s:s:0', calls[0])
        self.assertIn('title=English captions', calls[0])

    def test_explicit_video_only_selection_remains_valid(self):
        calls, _, output = self.publish({'acodec': 'none'}, [{'codec_type': 'video'}])
        self.assertEqual(calls, [])
        self.assertEqual(output, b'original')


if __name__ == '__main__':
    unittest.main()
