import type { Tool } from '../api';
import type { PageId } from '../pages';
import { ExplorePage, InsightsPage, NodesPage, SearchPage, StatusPage } from './graph';
import { MemoryPage, RulesPage, SessionsPage, SkillsPage } from './knowledge';
import { FilesPage, LoggingPage, OperationsPage, UsagePage } from './management';
import { ToolsPage } from './tools';

export function PageView({ page, tools }: { page: PageId; tools: readonly Tool[] }) {
  switch (page) {
    case 'status': return <StatusPage tools={tools} />;
    case 'search': return <SearchPage />;
    case 'nodes': return <NodesPage tools={tools} />;
    case 'explore': return <ExplorePage />;
    case 'insights': return <InsightsPage tools={tools} />;
    case 'rules': return <RulesPage />;
    case 'skills': return <SkillsPage />;
    case 'memory': return <MemoryPage tools={tools} />;
    case 'sessions': return <SessionsPage tools={tools} />;
    case 'operations': return <OperationsPage tools={tools} />;
    case 'files': return <FilesPage />;
    case 'usage': return <UsagePage tools={tools} />;
    case 'logging': return <LoggingPage />;
    case 'tools': return <ToolsPage tools={tools} />;
    default: {
      // A page added to `PageId` must be given a view here.
      const unknown: never = page;
      return unknown;
    }
  }
}
