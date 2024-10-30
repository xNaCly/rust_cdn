const rand = (Math.random() + 1).toString(36).substring(7);
console.log(
  await(
    await fetch("http://localhost:8080/file", {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: new URLSearchParams({
        name: `../${rand}.txt`,
        content: rand,
      }).toString(),
    })
  ).json()
);
console.log(
  await(await fetch(`http://localhost:8080/file/${rand}.txt`)).text()
);
