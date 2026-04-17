## Checklist

- [ ] I did not add new behavior to a legacy transition file.
- [ ] I did not put orchestration or read-model assembly into a transport module.
- [ ] I did not introduce a `frontend/src/shared/**` import from `app/**` or `features/**`.
- [ ] I preserved existing routes, websocket messages, and JSON payload fields unless the change explicitly required it.
