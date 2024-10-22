await fetch("http://localhost:8080/file", {
  method: "POST",
  headers: {
    "Content-Type": "application/x-www-form-urlencoded",
  },
  body: new URLSearchParams({
    filename: "helloWorld.txt",
  }).toString(),
});
