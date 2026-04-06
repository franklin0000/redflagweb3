const fs = require('fs').promises;
const path = require('path');

async function main() {
  const BASE_DIR = '/home/klk/.gemini/antigravity/scratch/redflag2.1/redflag-ui';
  const SRC_DIR = path.join(BASE_DIR, 'src');
  const COMPONENTS_DIR = path.join(SRC_DIR, 'components');
  const PAGES_DIR = path.join(SRC_DIR, 'pages');
  const HOME_DIR = path.join(PAGES_DIR, 'Home');

  try {
    await fs.mkdir(COMPONENTS_DIR, { recursive: true });
    await fs.mkdir(HOME_DIR, { recursive: true });

    // Navbar
    const navbarDir = path.join(COMPONENTS_DIR, 'Navbar');
    await fs.mkdir(navbarDir, { recursive: true });
    await fs.writeFile(path.join(navbarDir, 'Navbar.tsx'), 'export default () => <nav>Navbar</nav>;');

    // Footer
    const footerDir = path.join(COMPONENTS_DIR, 'Footer');
    await fs.mkdir(footerDir, { recursive: true });
    await fs.writeFile(path.join(footerDir, 'Footer.tsx'), 'export default () => <footer>Footer</footer>;');

    // Home Styles
    const homeStylesDir = path.join(HOME_DIR, 'styles');
    await fs.mkdir(homeStylesDir, { recursive: true });
    await fs.writeFile(path.join(homeStylesDir, 'HomeStyles.ts'), 'export const HomeWrapper = () => null;');

    // Home Index
    await fs.writeFile(path.join(HOME_DIR, 'index.tsx'), 'export default () => <div>Home</div>;');

    console.log('Dummy React structure created.');
  } catch (e) {
    console.error(e);
  }
}
main();
