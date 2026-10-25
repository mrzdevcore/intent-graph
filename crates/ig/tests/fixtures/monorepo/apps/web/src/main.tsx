import { Button, Card } from '@acme/ui';
import * as core from '@acme/core';
import { Repository } from '@acme/core';

const App = memo(() => <Card><Button n={core.add(1, 2)} /></Card>);
const r = new Repository();
r.find('x');
export default App;
