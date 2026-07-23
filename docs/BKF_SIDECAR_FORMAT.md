# BKF Prefix Sidecar v1

Magic:

```text
BKFPFX01
```

Header:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | Magic |
| 8 | 4 | Version |
| 12 | 4 | Page count |
| 16 | 32 | Source SHA-256 |
| 48 | 8 | Source size |
| 56 | 8 | Reserved |

Page record, 256 bytes:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | Page index |
| 4 | 8 | Segment offset |
| 12 | 8 | Segment length |
| 20 | 4 | Prefix length, must be 200 |
| 24 | 32 | Prefix SHA-256 |
| 56 | 200 | Decoded prefix |

ה־Sidecar אינו מפענח את הספר בעצמו. הוא מספק את 200 הבתים המפוענחים לכל
עמוד ומקטעי העמודים המאומתים.
