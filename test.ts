// console.log(
//     await (await fetch("http://localhost:8080/file", {
//         method: "POST",
//         headers: {
//             "Content-Type": "application/x-www-form-urlencoded",
//         },
//         body: new URLSearchParams({
//             name: `helloWorld-${
//                 (Math.random() + 1).toString(36).substring(7)
//             }.txt`,
//             content: "Hello World",
//         }).toString(),
//     })).json(),
// );
console.log(
    await (await fetch("http://localhost:8080/file/helloWorld-xl4rp.txt"))
        .json(),
);
