import express from "express";
const app = express();
export function handleUsers(req, res) {
    res.send("ok");
}
app.get("/users", handleUsers);
//# sourceMappingURL=server.js.map