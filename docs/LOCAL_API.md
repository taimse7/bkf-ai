# Local API

Base URL:

```text
http://127.0.0.1:47831/api/v1
```

כל endpoint מלבד `/health` דורש:

```http
Authorization: Bearer <local-token>
```

Endpoints:

- `GET /health`
- `GET /repositories`
- `GET /documents`
- `POST /search`
- `POST /documents/:id/prepare`
- `GET /documents/:id/pdf?token=<token>`

השרת מאזין ל־loopback בלבד ואינו מקבל נתיב קובץ חופשי. כל גישה מתבצעת לפי
`documentId` שנמצא ב־SQLite Catalog.

שרת ה־PDF תומך ב־HTTP `Range` כדי שה־Viewer לא יצטרך להוריד את כל הקובץ מראש.
