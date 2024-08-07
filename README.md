# Audio Transcoder

Сервис транскодирования аудиофайлов через HTTP API.

# Использование

Доступны следующие параметры:
- `format` - ожидаемый формат результата;
- `codec` - название кодека как в FFmpeg;
- `codec_opts` - опции кодека в формате `ключ1=значение1;ключ2=значение2`;
- `bit_rate` - битрейт целым числом;
- `max_bit_rate` - максимальный битрейт целым числом (для VBR);
- `sample_rate` - частота дискретизации целым числом;
- `channel_layout` - конфигурация каналов звука (обычно `mono` или `stereo`);
- `callback_url` - URL, на который будет отправлен вебхук после успеха или ошибки;
- `url` - URL файла для транскодирования;
- `file` - поле с содержимым транскодируемого файла.

Обязательные поля:
- `format`;
- `codec`;
- `sample_rate`;
- `file` (для метода `/enqueue`);
- `url` (для метода `/enqueue_url`).

Пример транскодирования с загрузкой файла напрямую в body:
```bash
curl --location 'http://localhost:8090/enqueue' \
--form 'file=@"/home/user/Music/test.mp3"' \
--form 'format="mp4"' \
--form 'codec="libfdk_aac"' \
--form 'codec_opts="profile=aac_he"' \
--form 'bit_rate="64000"' \
--form 'max_bit_rate="64000"' \
--form 'sample_rate="44100"' \
--form 'channel_layout="stereo"' \
--form 'callback_url="http://127.0.0.1:8909/callback"'
```

С загрузкой файла по указанному URL:
```bash
curl --location 'http://localhost:8090/enqueue_url' \
--header 'Content-Type: application/json' \
--data '{
    "format": "mp4",
    "codec": "libfdk_aac",
    "codec_opts": "profile=aac_he",
    "bit_rate": 64000,
    "max_bit_rate": 64000,
    "sample_rate": 44100,
    "channel_layout": "stereo",
    "url": "https://upload.wikimedia.org/wikipedia/commons/c/c8/Example.ogg",
    "callback_url": "http://127.0.0.1:8909/callback"
}'
```

После обработки джоба сервис отправит колбэк с результатом на указанный URL. Колбэк выглядит следующим образом:
```json
{
    "id": "73bf59be-c1bf-4476-8b0d-bb25837ec9df",
    "error": "couldn't find codec with name: hevc"
}
```

Если поле `error` пустое (`null`) - ошибки не произошло и файл успешно транскодирован.

Загрузить транскодированный файл можно следующим образом:
```bash
curl -L http://localhost:8090/get/73bf59be-c1bf-4476-8b0d-bb25837ec9df -o file.mp4
```

# Конфигурация

Доступны следующие переменные окружения:
- `LISTEN` - адрес HTTP-сервера. По умолчанию используется `0.0.0.0:8090`.
- `NUM_WORKERS` - указывает количество воркеров (потоков) для транскодирования входящих джобов. По умолчанию равно количеству логических ядер CPU.
- `TEMP_DIR` - указывает на директорию для хранения временных файлов (загруженных и результатов транскодирования). По умолчанию равно системной директории временных файлов (`/tmp` в Linux).
- `API_KEYS` - API-ключи для доступа к сервису (строка с элементами через запятую). Без указания этой переменной доступ будет без аутентификации.
- `LOG_LEVEL` - уровень логгирования, по умолчанию `info`.
- `MAX_BODY_SIZE` - максимальный размер тела входящего запроса для роута `/enqueue` и максимальный размер загружаемого для транскодирования файла запросом к `/enqueue_url`. По умолчанию равно 1GB (`file` в `/enqueue` имеет жестко заданный верхний лимит в `1GiB`).
- `RESULT_TTL_SEC` - время жизни результатов транскодирования в секундах, минимум - 60 секунд. Стандартное значение - 3600 (один час). После завершения этого периода результаты будут удалены из директории с временными файлами.
- `FFMPEG_VERBOSE` - при установке в `1` меняет уровень логгирования FFmpeg с `quiet` на `trace`.
