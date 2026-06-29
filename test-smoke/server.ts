import express from "express";
const app = express();

export function handleUsers(req: any, res: any) {
  res.send("ok");
}

app.get("/users", handleUsers);