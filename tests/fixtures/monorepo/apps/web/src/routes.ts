import express from 'express';
const app = express();
// @intent WEB-1
app.get('/users', (req, res) => { res.send(list()); });
app.post("/users", handler);
app.patch('/ignored', handler);
function list() { return []; }
function handler() {}
