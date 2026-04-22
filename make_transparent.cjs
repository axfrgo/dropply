const sharp = require('sharp');
const fs = require('fs');
const pngToIco = require('png-to-ico');

async function processIcon(filePath) {
    const metadata = await sharp(filePath).metadata();
    const w = metadata.width;
    const h = metadata.height;
    const radius = Math.min(w, h) / 2;
    const cx = w / 2;
    const cy = h / 2;

    const svgMask = `
        <svg width="${w}" height="${h}">
            <circle cx="${cx}" cy="${cy}" r="${radius}" fill="white" />
        </svg>
    `;

    const circleMask = Buffer.from(svgMask);

    const buf = await sharp(filePath)
        .composite([{ input: circleMask, blend: 'dest-in' }])
        .png()
        .toBuffer();

    fs.writeFileSync(filePath, buf);
    console.log('Processed', filePath);
}

async function main() {
    const files = [
        'src-tauri/icons/32x32.png',
        'src-tauri/icons/128x128.png',
        'src-tauri/icons/256x256.png'
    ];
    for (const f of files) {
        if (fs.existsSync(f)) {
            await processIcon(f);
        }
    }
    
    if (fs.existsSync('src-tauri/icons/256x256.png')) {
        const buf = await pngToIco(['src-tauri/icons/256x256.png']);
        fs.writeFileSync('src-tauri/icons/icon.ico', buf);
        console.log('Processed src-tauri/icons/icon.ico');
    }
}

main().catch(console.error);
