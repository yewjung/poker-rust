### Run backend server without Docker
1. start up docker-compose with the following command:
```bash
docker-compose up --build -d
```
2. stop the backend server with:
```bash
docker-compose stop <backend container id>
```
3. run the following command to start the server:
```bash
DATABASE_URL=postgres://user:password@localhost:5432/my_database RUST_LOG=debug cargo run
```