addEventListener("fetch", async (event) => {
    event.respondWith(handleRequest(event.request));
});

async function handleRequest(request) {
    let counter = await kv.test.get("counter");
    counter = (counter === null ? 0 : parseInt(counter)) + 1;

    await kv.test.put("counter", "" + counter);
    return new Response("New counter: " + counter);
}
