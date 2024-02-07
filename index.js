const fs = require('fs');
const yaml = require('js-yaml');
const puppeteer = require('puppeteer');
const { Worker, isMainThread, parentPort } = require('worker_threads');

if (isMainThread) {
    // Main thread
    const fileName = 'config_prod.yaml';

    try {
        // Read YAML file
        const yamlData = fs.readFileSync(fileName, 'utf8');

        // Parse YAML to JavaScript object
        const data = yaml.load(yamlData);

        // Do something with the data
        console.log(data.proxy_acc);
        const headless = data.headless;
        const proxy_addr = data.proxy_addr;
        const proxy_accs = data.proxy_acc;
        const download_url = data.download_url;

        for (let proxy_acc of proxy_accs) {
            proxy_acc = proxy_acc.split(",");
            const username = proxy_acc[0];
            const password = proxy_acc[1];

            const worker = new Worker(__filename);
            worker.postMessage({ username, password, proxy_addr, headless, download_url });
        }

    } catch (error) {
        console.error('Error reading YAML file:', error);
    }
} else {
    // Worker thread
    parentPort.once('message', async (message) => {
        try {
            const { username, password, proxy_addr, headless, download_url } = message;
            const browser = await puppeteer.launch({
                headless: headless,
                args: [`--proxy-server=${proxy_addr}`],
            });

            const page = await browser.newPage();

            await page.authenticate({ username, password });

            const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));

            while (true) {
                try {
                    console.log("navigating")
                    await page.goto(download_url);
                } catch {
                }
                await sleep(2000); // Sleep for secs
            }
        } catch { }

    });
}

