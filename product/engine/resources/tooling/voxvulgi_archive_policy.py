"""Product-owned yt-dlp preprocessor: best media and original/English captions."""
import os
import re
from urllib.parse import parse_qs, urlsplit

from yt_dlp.postprocessor.common import PostProcessor
from yt_dlp.postprocessor.ffmpeg import FFmpegPostProcessor
from yt_dlp.utils import PostProcessingError, ISO639Utils, prepend_extension


class VoxVulgiArchivePolicyPP(PostProcessor):
    def run(self, info):
        formats = info.get('formats') or [info]
        videos = [f for f in formats if f.get('vcodec') not in (None, 'none')
                  and not f.get('has_drm') and f.get('width') and f.get('height')]
        youtube = str(info.get('extractor_key', info.get('extractor', ''))).lower() == 'youtube'
        # A lone progressive format is an incomplete extraction, not evidence of best quality.
        if youtube and videos and all(str(f.get('format_id')) == '18' for f in videos):
            raise PostProcessingError('Quality check: YouTube exposed only its 360p fallback. '
                                      'No file was accepted; check downloader warnings and retry.')
        maximum = max((min(f['width'], f['height']) for f in videos), default=0)
        manual = {k: v for k, v in (info.get('subtitles') or {}).items() if k != 'live_chat'}
        automatic = info.get('automatic_captions') or {}
        originals = {k.removesuffix('-orig') for k in automatic if k.endswith('-orig')}
        for f in formats:
            if f.get('language') and (f.get('language_preference') or 0) > 0:
                originals.add(f['language'])
        if info.get('language'):
            originals.add(info['language'])
        def base(lang):
            return lang.lower().split('-')[0]
        wanted = {base(lang) for lang in originals} | {'en'}
        # Without original-language metadata retain authored tracks rather than guessing.
        selected_manual = {k: v for k, v in manual.items() if not originals or base(k) in wanted}
        selected_auto = {}
        labels = {}
        for language, tracks in selected_manual.items():
            labels[language] = 'authored'
        for language, tracks in automatic.items():
            lang = language.removesuffix('-orig')
            if base(lang) not in wanted or any(base(k) == base(lang) for k in selected_manual):
                continue
            # Prefer the explicitly original track over its duplicate/translated variant.
            key = lang + '-orig' if lang + '-orig' in automatic else language
            if lang in selected_auto:
                continue
            selected_auto[lang] = automatic[key]
            translated = any(parse_qs(urlsplit(t.get('url', '')).query).get('tlang')
                             for t in automatic[key])
            labels[lang] = 'automatic translation' if translated else 'automatic original'
        available = {**selected_auto, **selected_manual}
        self._downloader.params['subtitleslangs'] = [re.escape(k) for k in available]
        info['requested_subtitles'] = self._downloader.process_subtitles(
            info['id'], selected_manual, selected_auto) if available else None
        selected = info.get('requested_subtitles') or {}
        if youtube and any(labels.get(k) == 'automatic translation' for k in selected):
            # YouTube prepares translated tracks asynchronously after extraction.
            # Native/manual captions and videos without translations need no added delay.
            self._downloader.params['sleep_interval_subtitles'] = max(
                self._downloader.params.get('sleep_interval_subtitles') or 0, 60)
            self.to_screen('Waiting 60 seconds per subtitle for YouTube translated captions')
        for language, track in selected.items():
            name = track.get('name') or language.removesuffix('-orig')
            track['name'] = f'{name} [{labels.get(language, "source caption")}]'
        info['voxvulgi_archive_policy'] = {
            'schema_version': 1,
            'best_available_short_side': maximum,
            'original_languages': sorted(originals),
            'subtitles': [{'language': k, 'kind': labels.get(k, 'source caption'),
                           'title': selected[k]['name']}
                          for k in selected],
            'english_available': any(base(k) == 'en' for k in selected),
            'original_available': any(base(k) in {base(x) for x in originals} for k in selected)
                                  if originals else None,
        }
        return [], info


class VoxVulgiArchiveMetadataPP(FFmpegPostProcessor):
    """Publish selected stream labels after remuxing source containers into MKV."""
    @staticmethod
    def selected_audio_formats(info):
        downloads = info.get('requested_downloads')
        if downloads:
            if len(downloads) != 1:
                raise PostProcessingError('Archive audio metadata has ambiguous download mapping')
            selected = downloads[0].get('requested_formats') or [downloads[0]]
        else:
            selected = info.get('requested_formats') or [info]
        if any(not isinstance(f, dict) or not isinstance(f.get('acodec'), str) for f in selected):
            raise PostProcessingError('Archive audio metadata lacks explicit selected codec mapping')
        return [f for f in selected if f['acodec'].lower() != 'none']

    def run(self, info):
        subtitles = info.get('requested_subtitles') or {}
        filename = info['filepath']
        if info['ext'] != 'mkv':
            raise PostProcessingError('Archive metadata requires finalized MKV')
        audio = self.selected_audio_formats(info)
        observed = [s for s in self.get_metadata_object(filename)['streams']
                    if s.get('codec_type') == 'audio']
        # FFmpegMergerPP maps one audio stream per selected format, in selection order.
        # A progressive source is therefore safe only when it contains exactly one audio.
        if len(audio) != len(observed):
            raise PostProcessingError('Archive audio metadata selected/observed cardinality mismatch')
        options = ['-map', '0', '-c', 'copy']
        audio_receipt = []
        for index, selected in enumerate(audio):
            language = selected.get('language')
            language = language.strip() if isinstance(language, str) else None
            track = selected.get('audio_track') or {}
            title = track.get('display_name') if isinstance(track, dict) else None
            title = title.strip() if isinstance(title, str) else None
            if language:
                options.extend([f'-metadata:s:a:{index}',
                                'language=' + (ISO639Utils.short2long(language) or language)])
            if title:
                options.extend([f'-metadata:s:a:{index}', 'title=' + title])
            audio_receipt.append({'stream_index': index, 'format_id': selected.get('format_id'),
                                  'language': language or None, 'title': title or None})
        titles = {t['language']: t['title'] for t in
                  info.get('voxvulgi_archive_policy', {}).get('subtitles', [])}
        for index, (lang, track) in enumerate(subtitles.items()):
            track['name'] = titles.get(lang) or track.get('name') or lang
            options.extend([f'-metadata:s:s:{index}',
                            'language=' + (ISO639Utils.short2long(lang) or lang),
                            f'-metadata:s:s:{index}', 'title=' + (track.get('name') or lang)])
        info['voxvulgi_archive_audio_metadata'] = {'schema_version': 1, 'tracks': audio_receipt}
        if len(options) == 4:
            return [], info
        temporary = prepend_extension(filename, 'vv-metadata')
        self.run_ffmpeg(filename, temporary, options)
        os.replace(temporary, filename)
        return [], info
